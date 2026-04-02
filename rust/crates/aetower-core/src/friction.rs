use aetower_model::{EntitySnapshot, HostSnapshot};

pub fn apply(host: &HostSnapshot, entities: &mut [EntitySnapshot]) {
    let total_memory = host.memory_total_bytes.max(1) as f32;

    for entity in entities.iter_mut() {
        let cpu_score = (entity.metrics.cpu_percent / 100.0).min(2.0) * 45.0;
        let memory_score =
            (entity.metrics.memory_resident_bytes as f32 / total_memory).min(1.0) * 35.0;
        let disk_mib =
            (entity.metrics.disk_read_bps + entity.metrics.disk_write_bps) as f32 / 1_048_576.0;
        let disk_score = disk_mib.min(20.0) * 1.5;
        let foreground_bonus = if entity.metrics.is_foreground {
            10.0
        } else {
            0.0
        };

        let total_score = cpu_score + memory_score + disk_score + foreground_bonus;

        let mut reasons = Vec::new();
        if cpu_score > 15.0 {
            reasons.push(format!("high CPU {:.1}%", entity.metrics.cpu_percent));
        }
        if memory_score > 10.0 {
            reasons.push(format!(
                "high memory {:.1} MB",
                entity.metrics.memory_resident_bytes as f32 / 1_048_576.0
            ));
        }
        if disk_score > 8.0 {
            reasons.push(format!("heavy disk {:.1} MiB/s", disk_mib));
        }
        if entity.metrics.is_foreground {
            reasons.push("foreground app".to_owned());
        }
        if reasons.is_empty() {
            reasons.push("background baseline activity".to_owned());
        }

        entity.friction.total_score = total_score;
        entity.friction.cpu_score = cpu_score;
        entity.friction.memory_score = memory_score;
        entity.friction.disk_score = disk_score;
        entity.friction.foreground_bonus = foreground_bonus;
        entity.friction.reasons = reasons;
    }

    entities.sort_by(|left, right| {
        right
            .friction
            .total_score
            .partial_cmp(&left.friction.total_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| right.metrics.is_foreground.cmp(&left.metrics.is_foreground))
            .then_with(|| left.display_name.cmp(&right.display_name))
            .then_with(|| left.entity_id.cmp(&right.entity_id))
    });
}

#[cfg(test)]
mod tests {
    use aetower_model::{
        AggregateMetrics, EntityKind, EntitySnapshot, FrictionBreakdown, HostSnapshot, MetricTrend,
    };

    use super::apply;

    fn entity(id: &str, display_name: &str, metrics: AggregateMetrics) -> EntitySnapshot {
        EntitySnapshot {
            entity_id: id.to_owned(),
            display_name: display_name.to_owned(),
            primary_provenance: None,
            bundle_id: None,
            executable_path: None,
            oldest_process_start_millis: 0,
            newest_process_start_millis: 0,
            entity_kind: EntityKind::Service,
            metrics,
            friction: FrictionBreakdown::default(),
            components: Vec::new(),
            trend: MetricTrend::default(),
            badges: Vec::new(),
            active_window_title: None,
        }
    }

    #[test]
    fn baseline_activity_is_only_reason_when_no_hot_signals_exist() {
        let host = HostSnapshot {
            memory_total_bytes: 8 * 1024 * 1024 * 1024,
            ..HostSnapshot::default()
        };
        let mut entities = vec![entity("baseline", "baseline", AggregateMetrics::default())];

        apply(&host, &mut entities);

        assert_eq!(
            entities[0].friction.reasons,
            vec!["background baseline activity".to_owned()]
        );
    }

    #[test]
    fn friction_reasons_include_foreground_and_hot_metrics() {
        let host = HostSnapshot {
            memory_total_bytes: 8 * 1024 * 1024 * 1024,
            ..HostSnapshot::default()
        };
        let mut entities = vec![entity(
            "busy",
            "busy",
            AggregateMetrics {
                cpu_percent: 60.0,
                memory_resident_bytes: 3 * 1024 * 1024 * 1024,
                disk_read_bps: 12 * 1024 * 1024,
                is_foreground: true,
                ..AggregateMetrics::default()
            },
        )];

        apply(&host, &mut entities);

        assert!(entities[0]
            .friction
            .reasons
            .iter()
            .any(|reason| reason.contains("high CPU")));
        assert!(entities[0]
            .friction
            .reasons
            .iter()
            .any(|reason| reason.contains("high memory")));
        assert!(entities[0]
            .friction
            .reasons
            .iter()
            .any(|reason| reason.contains("heavy disk")));
        assert!(entities[0]
            .friction
            .reasons
            .iter()
            .any(|reason| reason == "foreground app"));
    }
}
