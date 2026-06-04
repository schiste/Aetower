use std::collections::BTreeMap;

use super::*;

pub(crate) fn build_entity_memory_breakdown(
    data_source: &dyn AetowerMcpDataSource,
    entity_id: &str,
    top_regions: usize,
) -> Result<EntityMemoryBreakdown, String> {
    let snapshot = data_source.latest_snapshot()?;
    let entity = snapshot
        .entities
        .iter()
        .find(|entity| entity.entity_id == entity_id)
        .ok_or_else(|| format!("Unknown entity_id: {entity_id}"))?;
    let process_ids = entity_process_ids(entity);
    if process_ids.is_empty() {
        return Err(format!(
            "Entity {} has no attributed process IDs to inspect.",
            entity.display_name
        ));
    }
    let regions = vmmap_regions_for_processes(&process_ids, top_regions.max(1))?;
    Ok(EntityMemoryBreakdown {
        captured_at_millis: snapshot.captured_at_millis,
        entity_id: entity.entity_id.clone(),
        display_name: entity.display_name.clone(),
        process_ids,
        resident_bytes: entity.metrics.memory_resident_bytes,
        physical_footprint_bytes: entity.metrics.memory_physical_footprint_bytes,
        memory_metric_note: if entity.metrics.memory_physical_footprint_bytes > 0 {
            "resident_bytes is the current resident set; physical_footprint_bytes follows macOS task footprint when available and may exceed resident because it includes graphics and other charged memory.".to_owned()
        } else {
            "resident_bytes is the current resident set; physical_footprint_bytes is unavailable for this entity on the current platform or sample.".to_owned()
        },
        regions,
    })
}

pub(crate) fn vmmap_regions_for_processes(
    process_ids: &[u32],
    top_regions: usize,
) -> Result<Vec<MemoryRegionBreakdown>, String> {
    let mut regions_by_type = BTreeMap::<String, MemoryRegionBreakdown>::new();
    for pid in process_ids {
        let output = run_os_command("/usr/bin/vmmap", &[pid.to_string()])?;
        for region in parse_vmmap_regions(&output) {
            let entry = regions_by_type.entry(region.region_type.clone()).or_insert(
                MemoryRegionBreakdown {
                    region_type: region.region_type.clone(),
                    virtual_bytes: 0,
                    resident_bytes: 0,
                    dirty_bytes: 0,
                    swap_bytes: 0,
                },
            );
            entry.virtual_bytes += region.virtual_bytes;
            entry.resident_bytes += region.resident_bytes;
            entry.dirty_bytes += region.dirty_bytes;
            entry.swap_bytes += region.swap_bytes;
        }
    }
    let mut regions = regions_by_type.into_values().collect::<Vec<_>>();
    regions.sort_by(|left, right| {
        right
            .resident_bytes
            .cmp(&left.resident_bytes)
            .then_with(|| right.virtual_bytes.cmp(&left.virtual_bytes))
    });
    regions.truncate(top_regions.max(1));
    Ok(regions)
}

pub fn memory_breakdown_json(
    data_source: &dyn AetowerMcpDataSource,
    entity_id: &str,
    top_regions: usize,
) -> Result<String, String> {
    let report = build_entity_memory_breakdown(data_source, entity_id, top_regions.max(1))?;
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

pub fn self_memory_attribution_json(
    data_source: &dyn AetowerMcpDataSource,
    top_regions: usize,
) -> Result<String, String> {
    let runtime = data_source.latest_runtime_lag_metrics()?;
    let samples = vec![self_runtime_watch_sample(&runtime)];
    let report =
        build_self_runtime_memory_attribution(data_source, &samples, true, top_regions.max(1));
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

pub(crate) fn parse_vmmap_regions(output: &str) -> Vec<MemoryRegionBreakdown> {
    output.lines().filter_map(parse_vmmap_region_line).collect()
}

pub(crate) fn parse_vmmap_region_line(line: &str) -> Option<MemoryRegionBreakdown> {
    let trimmed = line.trim_end();
    let bracket_start = trimmed.find('[')?;
    let bracket_end = trimmed[bracket_start..].find(']')? + bracket_start;
    let prefix = &trimmed[..bracket_start];
    let prefix_tokens = prefix.split_whitespace().collect::<Vec<_>>();
    let address_index = prefix_tokens.iter().position(|token| {
        token.contains('-')
            && token.split('-').all(|part| {
                !part.is_empty() && part.chars().all(|character| character.is_ascii_hexdigit())
            })
    })?;
    let region_type = prefix_tokens[..address_index].join(" ");
    if region_type.is_empty() || region_type == "REGION TYPE" {
        return None;
    }
    let stats = trimmed[bracket_start + 1..bracket_end]
        .split_whitespace()
        .collect::<Vec<_>>();
    if stats.len() < 4 {
        return None;
    }
    Some(MemoryRegionBreakdown {
        region_type,
        virtual_bytes: parse_vmmap_bytes(stats[0]),
        resident_bytes: parse_vmmap_bytes(stats[1]),
        dirty_bytes: parse_vmmap_bytes(stats[2]),
        swap_bytes: parse_vmmap_bytes(stats[3]),
    })
}

pub(crate) fn parse_vmmap_bytes(value: &str) -> u64 {
    let numeric = value.trim_end_matches(|character: char| character.is_ascii_alphabetic());
    let suffix = value[numeric.len()..].to_ascii_uppercase();
    let number = numeric.parse::<f64>().unwrap_or(0.0);
    let multiplier = match suffix.as_str() {
        "K" => 1024.0,
        "M" => 1024.0 * 1024.0,
        "G" => 1024.0 * 1024.0 * 1024.0,
        "T" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => 1.0,
    };
    (number * multiplier).round() as u64
}
