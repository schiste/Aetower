use aetower_model::{EntitySnapshot, HostSnapshot, Recommendation};

pub fn apply(host: &HostSnapshot, entities: &mut [EntitySnapshot]) {
    let total_memory = host.memory_total_bytes.max(1) as f32;
    let host_network_bps = host
        .network_receive_bps
        .saturating_add(host.network_send_bps)
        .max(1) as f32;
    let host_pressure_factor = pressure_factor(host);
    let thermal_multiplier = thermal_multiplier(host.thermal_state.as_str());
    let battery_multiplier = if host.on_battery { 1.08 } else { 1.0 };

    for entity in entities.iter_mut() {
        let cpu_score = (entity.metrics.cpu_percent / 100.0).min(2.0) * 36.0;
        let memory_score =
            (entity.metrics.memory_resident_bytes as f32 / total_memory).min(1.0) * 26.0;
        let disk_mib =
            (entity.metrics.disk_read_bps + entity.metrics.disk_write_bps) as f32 / 1_048_576.0;
        let disk_score = disk_mib.min(20.0) * 1.2;

        let network_bps = entity
            .metrics
            .network_receive_bps
            .saturating_add(entity.metrics.network_send_bps);
        let network_mib = network_bps as f32 / 1_048_576.0;
        let network_share = (network_bps as f32 / host_network_bps).min(1.0);
        let network_score = ((network_mib / 8.0).min(1.0) * 10.0) + (network_share * 8.0);

        let wakeups_score = (entity.metrics.wakeups_per_second / 500.0).min(1.0) * 8.0;
        let pressure_score = host_pressure_factor
            * (entity.metrics.memory_resident_bytes as f32 / total_memory)
            * 20.0;
        let foreground_bonus = if entity.metrics.is_foreground {
            10.0
        } else {
            0.0
        };

        let total_score = (cpu_score
            + memory_score
            + disk_score
            + network_score
            + wakeups_score
            + pressure_score
            + foreground_bonus)
            * thermal_multiplier
            * battery_multiplier;

        let mut reasons = Vec::new();
        if cpu_score > 14.0 {
            reasons.push(format!("high CPU {:.1}%", entity.metrics.cpu_percent));
        }
        if memory_score > 8.0 {
            reasons.push(format!(
                "high memory {:.1} MB",
                entity.metrics.memory_resident_bytes as f32 / 1_048_576.0
            ));
        }
        if pressure_score > 4.0 && host_pressure_factor > 0.15 {
            reasons.push(format!(
                "memory pressure with {:.1} GB compressed",
                host.compressed_memory_bytes as f32 / 1_073_741_824.0
            ));
        }
        if disk_score > 7.0 {
            reasons.push(format!("heavy disk {:.1} MiB/s", disk_mib));
        }
        if network_score > 7.0 {
            reasons.push(format!("heavy network {:.1} MiB/s", network_mib));
        }
        if wakeups_score > 3.0 {
            reasons.push(format!(
                "wakeups {:.0}/s",
                entity.metrics.wakeups_per_second
            ));
        }
        if entity.metrics.is_foreground {
            reasons.push("foreground app".to_owned());
        }
        if reasons.is_empty() {
            reasons.push("background baseline activity".to_owned());
        }

        // Energy impact: weighted combination of power-hungry components.
        // Scale: 0-100 where 100 = maximum single-entity battery drain.
        // Memory contributes via compressor/swap I/O; network via radio activity.
        let thermal_penalty = (thermal_multiplier - 1.0) * 200.0;
        let battery_penalty = if host.on_battery { 10.0 } else { 0.0 };
        let energy_impact_score = (cpu_score * 0.40
            + memory_score * 0.10
            + wakeups_score * 0.15
            + disk_score * 0.10
            + network_score * 0.10
            + thermal_penalty * 0.10
            + battery_penalty * 0.05)
            .min(100.0);

        entity.friction.total_score = total_score;
        entity.friction.cpu_score = cpu_score;
        entity.friction.memory_score = memory_score;
        entity.friction.disk_score = disk_score;
        entity.friction.network_score = network_score;
        entity.friction.wakeups_score = wakeups_score;
        entity.friction.pressure_score = pressure_score;
        entity.friction.foreground_bonus = foreground_bonus;
        entity.friction.energy_impact_score = energy_impact_score;
        entity.friction.reasons = reasons;
        entity.recommendations = recommendations_for_entity(entity, host, network_mib);
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

fn pressure_factor(host: &HostSnapshot) -> f32 {
    let compressed_ratio = if host.memory_total_bytes == 0 {
        0.0
    } else {
        host.compressed_memory_bytes as f32 / host.memory_total_bytes as f32
    };
    let swap_ratio = if host.memory_total_bytes == 0 {
        0.0
    } else {
        host.swap_used_bytes as f32 / host.memory_total_bytes as f32
    };
    (compressed_ratio * 1.5 + swap_ratio * 2.0).min(1.0)
}

fn thermal_multiplier(state: &str) -> f32 {
    match state {
        "critical" => 1.18,
        "serious" => 1.12,
        "fair" => 1.05,
        _ => 1.0,
    }
}

fn recommendations_for_entity(
    entity: &EntitySnapshot,
    host: &HostSnapshot,
    network_mib: f32,
) -> Vec<Recommendation> {
    let mut recommendations = Vec::new();

    if entity.friction.cpu_score > 14.0 {
        recommendations.push(Recommendation {
            title: "Reduce active compute".to_owned(),
            detail: if entity.metrics.is_foreground {
                "This app is burning sustained CPU in the foreground. Pause the active task, close the busiest window, or let the run finish before switching back.".to_owned()
            } else {
                "This background workload is consuming meaningful CPU. Pause auto-refresh, background jobs, or extension activity if the Mac feels busy.".to_owned()
            },
        });
    }

    if entity.friction.pressure_score > 4.0 || entity.friction.memory_score > 8.0 {
        recommendations.push(Recommendation {
            title: "Relieve memory pressure".to_owned(),
            detail: format!(
                "This entity is a meaningful share of memory while the Mac is carrying {:.1} GB compressed and {:.1} GB swap. Close heavy tabs, large workspaces, or restart the app if memory keeps climbing.",
                host.compressed_memory_bytes as f32 / 1_073_741_824.0,
                host.swap_used_bytes as f32 / 1_073_741_824.0
            ),
        });
    }

    if entity.friction.network_score > 7.0 {
        recommendations.push(Recommendation {
            title: "Pause network-heavy work".to_owned(),
            detail: format!(
                "This entity is pushing about {:.1} MiB/s of network traffic. Pause sync, downloads, uploads, or remote dev sessions if responsiveness matters right now.",
                network_mib
            ),
        });
    }

    if entity.friction.wakeups_score > 4.0 {
        recommendations.push(Recommendation {
            title: "Look for timer churn".to_owned(),
            detail: "Frequent wakeups usually come from watchers, polling loops, extensions, or background refresh timers. Disable the noisiest background feature first.".to_owned(),
        });
    }

    if entity.badges.iter().any(|badge| badge == "vscode-live") {
        recommendations.push(Recommendation {
            title: "Trim VS Code background load".to_owned(),
            detail: "Check extension hosts, file watchers, terminals, and workspace tasks. Large workspaces or noisy extensions often dominate Code-related friction.".to_owned(),
        });
    } else if entity.badges.iter().any(|badge| badge == "chromium-live") {
        recommendations.push(Recommendation {
            title: "Inspect the busiest tab".to_owned(),
            detail: "Browser helper load is often dominated by one active tab or extension. Use the component list to identify the loudest tab first.".to_owned(),
        });
    }

    recommendations.truncate(3);
    recommendations
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
            anomaly_detected: false,
            thermal_contribution: None,
            grouping_suggestion: None,
            agent_cost: None,
            session_markers: Vec::new(),
            recommendations: Vec::new(),
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
        assert!(entities[0].recommendations.is_empty());
    }

    #[test]
    fn friction_reasons_include_new_core_signals() {
        let host = HostSnapshot {
            memory_total_bytes: 8 * 1024 * 1024 * 1024,
            compressed_memory_bytes: 4 * 1024 * 1024 * 1024,
            swap_used_bytes: 2 * 1024 * 1024 * 1024,
            network_receive_bps: 20 * 1024 * 1024,
            network_send_bps: 2 * 1024 * 1024,
            ..HostSnapshot::default()
        };
        let mut entities = vec![entity(
            "busy",
            "busy",
            AggregateMetrics {
                cpu_percent: 60.0,
                memory_resident_bytes: 3 * 1024 * 1024 * 1024,
                disk_read_bps: 12 * 1024 * 1024,
                network_receive_bps: 10 * 1024 * 1024,
                wakeups_per_second: 220.0,
                is_foreground: true,
                ..AggregateMetrics::default()
            },
        )];

        apply(&host, &mut entities);

        assert!(
            entities[0]
                .friction
                .reasons
                .iter()
                .any(|reason| reason.contains("high CPU"))
        );
        assert!(
            entities[0]
                .friction
                .reasons
                .iter()
                .any(|reason| reason.contains("high memory"))
        );
        assert!(
            entities[0]
                .friction
                .reasons
                .iter()
                .any(|reason| reason.contains("memory pressure"))
        );
        assert!(
            entities[0]
                .friction
                .reasons
                .iter()
                .any(|reason| reason.contains("heavy disk"))
        );
        assert!(
            entities[0]
                .friction
                .reasons
                .iter()
                .any(|reason| reason.contains("heavy network"))
        );
        assert!(
            entities[0]
                .friction
                .reasons
                .iter()
                .any(|reason| reason.contains("wakeups"))
        );
        assert!(!entities[0].recommendations.is_empty());
    }
}
