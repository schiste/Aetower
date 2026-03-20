use std::collections::BTreeMap;

use aetower_model::{
    AggregateMetrics, ComponentKind, ComponentSnapshot, EntitySnapshot, FrontmostAppState,
};

use crate::{
    collector::RawProcessSample,
    identity::{EntitySeed, IdentityMap},
};

pub fn build_entities(
    processes: &[RawProcessSample],
    identity: &IdentityMap,
    frontmost: Option<&FrontmostAppState>,
) -> Vec<EntitySnapshot> {
    let mut grouped: BTreeMap<String, EntitySnapshot> = BTreeMap::new();

    for process in processes {
        let Some(entity_id) = identity.pid_to_entity.get(&process.pid) else {
            continue;
        };
        let Some(seed) = identity.entities.get(entity_id) else {
            continue;
        };

        let entry = grouped
            .entry(entity_id.clone())
            .or_insert_with(|| entity_from_seed(seed));

        entry.metrics.cpu_percent += process.cpu_percent;
        entry.metrics.memory_resident_bytes =
            entry.metrics.memory_resident_bytes.max(process.memory_bytes);
        entry.metrics.virtual_memory_bytes = entry
            .metrics
            .virtual_memory_bytes
            .max(process.virtual_memory_bytes);
        entry.metrics.disk_read_bps += process.disk_read_bytes;
        entry.metrics.disk_write_bps += process.disk_write_bytes;
        entry.metrics.process_count += 1;
        entry.metrics.is_foreground =
            entry.metrics.is_foreground || is_foreground_match(seed, frontmost);
        if entry.metrics.is_foreground {
            entry.active_window_title = frontmost.and_then(|state| state.window_title.clone());
        }

        entry.components.push(ComponentSnapshot {
            kind: if seed.entity_kind == aetower_model::EntityKind::TerminalSession {
                ComponentKind::Command
            } else {
                ComponentKind::Process
            },
            title: process.name.clone(),
            detail: summarize_process(process),
            cpu_percent: process.cpu_percent,
            memory_bytes: process.memory_bytes,
        });
    }

    let mut entities: Vec<_> = grouped.into_values().collect();
    for entity in &mut entities {
        entity.components.sort_by(|left, right| {
            right
                .cpu_percent
                .partial_cmp(&left.cpu_percent)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| right.memory_bytes.cmp(&left.memory_bytes))
                .then_with(|| left.title.cmp(&right.title))
        });
        entity.components.truncate(8);
    }
    entities
}

fn entity_from_seed(seed: &EntitySeed) -> EntitySnapshot {
    EntitySnapshot {
        entity_id: seed.entity_id.clone(),
        display_name: seed.display_name.clone(),
        bundle_id: seed.bundle_id.clone(),
        executable_path: seed.executable_path.clone(),
        entity_kind: seed.entity_kind.clone(),
        metrics: AggregateMetrics::default(),
        friction: Default::default(),
        components: Vec::new(),
        trend: Default::default(),
        badges: seed.badges.clone(),
        active_window_title: None,
    }
}

fn summarize_process(process: &RawProcessSample) -> String {
    if process.cmd.is_empty() {
        format!("pid {}", process.pid)
    } else {
        process.cmd.join(" ")
    }
}

fn is_foreground_match(seed: &EntitySeed, frontmost: Option<&FrontmostAppState>) -> bool {
    let Some(frontmost) = frontmost else {
        return matches!(
            seed.entity_kind,
            aetower_model::EntityKind::App
                | aetower_model::EntityKind::Browser
                | aetower_model::EntityKind::TerminalSession
        );
    };

    if let (Some(seed_bundle), Some(frontmost_bundle)) =
        (seed.bundle_id.as_deref(), frontmost.bundle_id.as_deref())
    {
        if seed_bundle == frontmost_bundle {
            return true;
        }
    }

    if let (Some(seed_path), Some(frontmost_path)) =
        (seed.executable_path.as_deref(), frontmost.executable_path.as_deref())
    {
        if seed_path == frontmost_path {
            return true;
        }
    }

    seed.display_name == frontmost.app_name
}
