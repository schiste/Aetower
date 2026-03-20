use std::collections::BTreeMap;

use aetower_model::{AggregateMetrics, ComponentKind, ComponentSnapshot, EntitySnapshot};

use crate::{
    collector::RawProcessSample,
    identity::{EntitySeed, IdentityMap},
};

pub fn build_entities(processes: &[RawProcessSample], identity: &IdentityMap) -> Vec<EntitySnapshot> {
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
        entry.metrics.memory_resident_bytes += process.memory_bytes;
        entry.metrics.virtual_memory_bytes += process.virtual_memory_bytes;
        entry.metrics.disk_read_bps += process.disk_read_bytes;
        entry.metrics.disk_write_bps += process.disk_write_bytes;
        entry.metrics.process_count += 1;
        entry.metrics.is_foreground = entry.metrics.is_foreground || is_foreground_candidate(seed);

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
        badges: seed.badges.clone(),
    }
}

fn summarize_process(process: &RawProcessSample) -> String {
    if process.cmd.is_empty() {
        format!("pid {}", process.pid)
    } else {
        process.cmd.join(" ")
    }
}

fn is_foreground_candidate(seed: &EntitySeed) -> bool {
    matches!(
        seed.entity_kind,
        aetower_model::EntityKind::App
            | aetower_model::EntityKind::Browser
            | aetower_model::EntityKind::TerminalSession
    )
}
