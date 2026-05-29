use aetower_model::{
    EntitySnapshot, FrictionContributor, HostSnapshot, Recommendation, ThermalState,
    entity_effective_memory_bytes, host_memory_pressure_factor,
};
use smallvec::SmallVec;

pub fn apply(host: &HostSnapshot, entities: &mut [EntitySnapshot]) {
    let total_memory = host.memory_total_bytes.max(1) as f32;
    let host_network_bps = host
        .network_receive_bps
        .saturating_add(host.network_send_bps)
        .max(1) as f32;
    let host_pressure_factor = pressure_factor(host);
    let thermal_multiplier = thermal_multiplier(host.thermal_state);
    let battery_multiplier = if host.on_battery { 1.08 } else { 1.0 };

    for entity in entities.iter_mut() {
        let effective_memory_bytes = entity_effective_memory_bytes(&entity.metrics) as f32;
        let cpu_score = (entity.metrics.cpu_percent / 100.0).min(2.0) * 36.0;
        let memory_score = (effective_memory_bytes / total_memory).min(1.0) * 26.0;
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
        let pressure_score = host_pressure_factor * (effective_memory_bytes / total_memory) * 20.0;
        let foreground_bonus = if entity.metrics.is_foreground {
            10.0
        } else {
            0.0
        };

        // Real per-process energy from the kernel when available (macOS
        // M-series, process running long enough for a non-zero delta).
        // Log-scale normalisation: 1 mW → 0, 100 mW → ~33, 1 W → ~66,
        // 10 W → 100.  When energy is 0 (kernel not reporting, process
        // just spawned, or non-macOS), this contributes nothing and the
        // existing heuristic terms carry the full weight.
        let energy_score = if entity.metrics.energy_nj_per_s > 0.0 {
            let log_energy = (entity.metrics.energy_nj_per_s.log10() - 6.0).clamp(0.0, 4.0);
            (log_energy / 4.0 * 100.0) as f32
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

        let mut reasons = SmallVec::<[String; 3]>::new();
        if cpu_score > 14.0 {
            reasons.push(format!("high CPU {:.1}%", entity.metrics.cpu_percent));
        }
        if memory_score > 8.0 {
            reasons.push(format!(
                "high memory {:.1} MB",
                effective_memory_bytes / 1_048_576.0
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
        if energy_score > 10.0 {
            let mw = entity.metrics.energy_nj_per_s / 1_000_000.0;
            if mw >= 1000.0 {
                reasons.push(format!("energy {:.1} W", mw / 1000.0));
            } else {
                reasons.push(format!("energy {:.0} mW", mw));
            }
        }
        if entity.metrics.is_foreground {
            reasons.push("foreground app".to_owned());
        }
        if reasons.is_empty() {
            reasons.push("background baseline activity".to_owned());
        }

        // Energy impact: when the kernel reports real per-process energy
        // (`energy_score > 0`), weight it alongside the existing heuristic
        // terms so the score reflects actual measured energy draw. CPU
        // weight is reduced from 0.40 to 0.30 to make room without
        // double-counting (CPU correlates strongly with energy).
        let thermal_penalty = (thermal_multiplier - 1.0) * 200.0;
        let battery_penalty = if host.on_battery { 10.0 } else { 0.0 };
        let energy_impact_score = if energy_score > 0.0 {
            (cpu_score * 0.30
                + energy_score * 0.10
                + memory_score * 0.10
                + wakeups_score * 0.15
                + disk_score * 0.10
                + network_score * 0.10
                + thermal_penalty * 0.10
                + battery_penalty * 0.05)
                .min(100.0)
        } else {
            (cpu_score * 0.40
                + memory_score * 0.10
                + wakeups_score * 0.15
                + disk_score * 0.10
                + network_score * 0.10
                + thermal_penalty * 0.10
                + battery_penalty * 0.05)
                .min(100.0)
        };

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
        entity.friction.contributors = friction_contributors(
            entity,
            host,
            cpu_score,
            memory_score,
            disk_score,
            network_score,
            wakeups_score,
            pressure_score,
            foreground_bonus,
            energy_score,
            disk_mib,
            network_mib,
        );
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

#[allow(clippy::too_many_arguments)]
fn friction_contributors(
    entity: &EntitySnapshot,
    host: &HostSnapshot,
    cpu_score: f32,
    memory_score: f32,
    disk_score: f32,
    network_score: f32,
    wakeups_score: f32,
    pressure_score: f32,
    foreground_bonus: f32,
    energy_score: f32,
    disk_mib: f32,
    network_mib: f32,
) -> Vec<FrictionContributor> {
    let mut contributors = Vec::with_capacity(6);
    if cpu_score > 0.0 {
        contributors.push(FrictionContributor {
            key: "cpu".to_owned(),
            label: "CPU load".to_owned(),
            score: cpu_score,
            detail: format!(
                "{:.1}% active CPU across grouped processes",
                entity.metrics.cpu_percent
            ),
        });
    }
    if memory_score > 0.0 {
        contributors.push(FrictionContributor {
            key: "memory".to_owned(),
            label: "Memory footprint".to_owned(),
            score: memory_score,
            detail: format!(
                "{:.1} MB charged memory, {:.1}% of host memory",
                entity_effective_memory_bytes(&entity.metrics) as f32 / 1_048_576.0,
                if host.memory_total_bytes == 0 {
                    0.0
                } else {
                    (entity_effective_memory_bytes(&entity.metrics) as f32
                        / host.memory_total_bytes as f32)
                        * 100.0
                }
            ),
        });
    }
    if pressure_score > 0.0 {
        contributors.push(FrictionContributor {
            key: "pressure".to_owned(),
            label: "Memory pressure".to_owned(),
            score: pressure_score,
            detail: format!(
                "{:.1} GB compressed, {:.1} GB swap in use on the host",
                host.compressed_memory_bytes as f32 / 1_073_741_824.0,
                host.swap_used_bytes as f32 / 1_073_741_824.0
            ),
        });
    }
    if disk_score > 0.0 {
        contributors.push(FrictionContributor {
            key: "disk".to_owned(),
            label: "Disk activity".to_owned(),
            score: disk_score,
            detail: format!("{disk_mib:.1} MiB/s read + write throughput"),
        });
    }
    if network_score > 0.0 {
        contributors.push(FrictionContributor {
            key: "network".to_owned(),
            label: "Network activity".to_owned(),
            score: network_score,
            detail: format!("{network_mib:.1} MiB/s receive + send throughput"),
        });
    }
    if wakeups_score > 0.0 {
        contributors.push(FrictionContributor {
            key: "wakeups".to_owned(),
            label: "Wakeups".to_owned(),
            score: wakeups_score,
            detail: format!(
                "{:.0} wakeups per second",
                entity.metrics.wakeups_per_second
            ),
        });
    }
    if energy_score > 0.0 {
        let mw = entity.metrics.energy_nj_per_s / 1_000_000.0;
        let label = if mw >= 1000.0 {
            format!("{:.1} W measured", mw / 1000.0)
        } else {
            format!("{:.0} mW measured", mw)
        };
        contributors.push(FrictionContributor {
            key: "energy".to_owned(),
            label: "Energy draw".to_owned(),
            score: energy_score,
            detail: label,
        });
    }
    if foreground_bonus > 0.0 {
        contributors.push(FrictionContributor {
            key: "foreground".to_owned(),
            label: "Foreground priority".to_owned(),
            score: foreground_bonus,
            detail: "This entity is frontmost, so Aetower biases it upward as the likely cause of visible friction.".to_owned(),
        });
    }

    contributors.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.label.cmp(&right.label))
    });
    contributors.truncate(5);
    contributors
}

fn pressure_factor(host: &HostSnapshot) -> f32 {
    host_memory_pressure_factor(host)
}

fn thermal_multiplier(state: ThermalState) -> f32 {
    match state {
        ThermalState::Critical => 1.18,
        ThermalState::Serious => 1.12,
        ThermalState::Fair => 1.05,
        ThermalState::Nominal => 1.0,
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
            launcher_summary: None,
            attribution_notes: Vec::new(),
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
            recent_change_summary: None,
            anomaly_detected: false,
            thermal_contribution: None,
            grouping_suggestion: None,
            agent_cost: None,
            session_markers: Vec::new(),
            recommendations: Vec::new(),
            network_connections: Vec::new(),
            signing_classification: "unknown".to_owned(),
            is_adhoc: false,
            binary_reputation: None,
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
            entities[0].friction.reasons.as_slice(),
            ["background baseline activity".to_owned()]
        );
        assert!(entities[0].friction.contributors.is_empty());
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
                memory_physical_footprint_bytes: 0,
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
        assert_eq!(entities[0].friction.contributors[0].key, "cpu");
        assert!(entities[0].friction.contributors.len() <= 5);
        assert!(
            entities[0]
                .friction
                .contributors
                .iter()
                .any(|contributor| contributor.key == "disk")
        );
        assert!(
            entities[0]
                .friction
                .contributors
                .iter()
                .any(|contributor| contributor.key == "network")
        );
        assert!(!entities[0].recommendations.is_empty());
    }

    /// Regression: when the kernel reports real per-process energy
    /// (energy_nj_per_s > 0), the friction scorer must:
    /// 1. Compute a log-scale energy_score (1 W = ~66)
    /// 2. Use the energy-present weight branch (CPU 0.30 + energy 0.10)
    /// 3. Include "energy" in reasons and contributors
    #[test]
    fn real_energy_data_produces_energy_friction_contributor() {
        let host = HostSnapshot {
            memory_total_bytes: 16 * 1024 * 1024 * 1024,
            ..HostSnapshot::default()
        };
        // 1 W = 1_000_000_000 nJ/s → log10 = 9.0 → (9.0 - 6.0) / 4 * 100 = 75.0
        let mut entities = vec![entity(
            "heavy",
            "heavy",
            AggregateMetrics {
                cpu_percent: 50.0,
                memory_resident_bytes: 512 * 1024 * 1024,
                energy_nj_per_s: 1_000_000_000.0, // 1 watt
                ..AggregateMetrics::default()
            },
        )];

        apply(&host, &mut entities);

        // energy_impact_score should use the energy-present branch
        assert!(
            entities[0].friction.energy_impact_score > 0.0,
            "energy_impact_score must be non-zero when real energy is reported"
        );
        // "energy" must appear in reasons
        assert!(
            entities[0]
                .friction
                .reasons
                .iter()
                .any(|reason| reason.contains("energy")),
            "friction reasons must mention energy: {:?}",
            entities[0].friction.reasons
        );
        // "energy" must appear in contributors
        let energy_contributor = entities[0]
            .friction
            .contributors
            .iter()
            .find(|contributor| contributor.key == "energy");
        assert!(
            energy_contributor.is_some(),
            "energy contributor must be present: {:?}",
            entities[0].friction.contributors
        );
        let contributor = energy_contributor.unwrap_or_else(|| {
            panic!("checked above");
        });
        // 1 W at log scale → score ~75
        assert!(
            contributor.score > 50.0 && contributor.score < 100.0,
            "1 W should produce energy_score in 50-100 range, got {}",
            contributor.score
        );
        assert!(
            contributor.detail.contains("W"),
            "detail should mention watts: {}",
            contributor.detail
        );
    }

    /// When energy_nj_per_s is 0 (kernel not reporting), the fallback
    /// heuristic path must be taken and no energy contributor should
    /// appear.
    #[test]
    fn zero_energy_uses_fallback_heuristic() {
        let host = HostSnapshot {
            memory_total_bytes: 16 * 1024 * 1024 * 1024,
            ..HostSnapshot::default()
        };
        let mut entities = vec![entity(
            "no-energy",
            "no-energy",
            AggregateMetrics {
                cpu_percent: 50.0,
                energy_nj_per_s: 0.0,
                ..AggregateMetrics::default()
            },
        )];

        apply(&host, &mut entities);

        assert!(
            !entities[0]
                .friction
                .contributors
                .iter()
                .any(|contributor| contributor.key == "energy"),
            "energy contributor must NOT appear when energy_nj_per_s is zero"
        );
    }
}
