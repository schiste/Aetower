//! Diagnostics-domain report builders for the MCP server.

use std::collections::BTreeMap;

use aetower_diagnostics::{
    DiagnosticsEvent, DiagnosticsLevel, DiagnosticsOverview, DiagnosticsSubsystem,
};
use aetower_model::SystemSnapshot;
use serde_json::Value;

use crate::*;

pub(crate) fn build_anomaly_explanations(
    snapshot: &SystemSnapshot,
    requested_entity_ids: &[String],
    limit: usize,
    window_millis: u64,
) -> Vec<AnomalyExplanation> {
    let requested = requested_entity_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut entities = snapshot
        .entities
        .iter()
        .filter(|entity| {
            if !requested.is_empty() {
                return requested.contains(&entity.entity_id);
            }
            entity.anomaly_detected || entity.recent_change_summary.is_some()
        })
        .collect::<Vec<_>>();
    entities.sort_by(|left, right| {
        right
            .friction
            .total_score
            .partial_cmp(&left.friction.total_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    entities.truncate(limit.max(1));
    let since = snapshot.captured_at_millis.saturating_sub(window_millis);

    entities
        .into_iter()
        .map(|entity| {
            let mut drivers = entity_metric_drivers(entity);
            drivers.sort_by(|left, right| {
                right
                    .delta
                    .abs()
                    .partial_cmp(&left.delta.abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            drivers.truncate(3);
            let supporting_events = snapshot
                .timeline
                .iter()
                .filter(|event| {
                    event.timestamp_millis >= since
                        && event.entity_id.as_deref() == Some(entity.entity_id.as_str())
                })
                .map(recent_change_from_timeline_event)
                .take(3)
                .collect::<Vec<_>>();
            let driver_summary = drivers
                .iter()
                .map(|driver| driver.summary.clone())
                .collect::<Vec<_>>()
                .join(" ");
            AnomalyExplanation {
                entity_id: entity.entity_id.clone(),
                display_name: entity.display_name.clone(),
                severity: if entity.friction.total_score >= 20.0 || entity.anomaly_detected {
                    SeverityBand::Warning
                } else {
                    SeverityBand::Info
                },
                summary: if driver_summary.is_empty() {
                    entity.recent_change_summary.clone().unwrap_or_else(|| {
                        "This entity is unusual relative to its recent trend.".to_owned()
                    })
                } else {
                    driver_summary
                },
                recent_change_summary: entity.recent_change_summary.clone(),
                drivers,
                supporting_events,
            }
        })
        .collect()
}

pub fn explain_anomalies_json(
    data_source: &dyn AetowerMcpDataSource,
    entity_ids: &[String],
    limit: usize,
    window_minutes: u64,
) -> Result<String, String> {
    let snapshot = data_source.latest_snapshot()?;
    let explanations = build_anomaly_explanations(
        &snapshot,
        entity_ids,
        limit.max(1),
        window_minutes.max(1) * 60_000,
    );
    serde_json::to_string(&explanations).map_err(|error| error.to_string())
}

pub(crate) fn entity_metric_drivers(entity: &aetower_model::EntitySnapshot) -> Vec<AnomalyDriver> {
    let mut drivers = Vec::new();
    push_driver(
        &mut drivers,
        "friction",
        entity
            .trend
            .friction
            .iter()
            .map(|value| *value as f64)
            .collect::<Vec<_>>(),
    );
    push_driver(
        &mut drivers,
        "cpu_percent",
        entity
            .trend
            .cpu_percent
            .iter()
            .map(|value| *value as f64)
            .collect::<Vec<_>>(),
    );
    push_driver(
        &mut drivers,
        "memory_bytes",
        entity
            .trend
            .memory_resident_bytes
            .iter()
            .map(|value| *value as f64)
            .collect::<Vec<_>>(),
    );
    push_driver(
        &mut drivers,
        "wakeups_per_second",
        entity
            .trend
            .wakeups_per_second
            .iter()
            .map(|value| *value as f64)
            .collect::<Vec<_>>(),
    );
    push_driver(
        &mut drivers,
        "disk_activity_bps",
        entity
            .trend
            .disk_activity_bps
            .iter()
            .map(|value| *value as f64)
            .collect::<Vec<_>>(),
    );
    push_driver(
        &mut drivers,
        "network_activity_bps",
        entity
            .trend
            .network_activity_bps
            .iter()
            .map(|value| *value as f64)
            .collect::<Vec<_>>(),
    );
    drivers
}

pub(crate) fn push_driver(drivers: &mut Vec<AnomalyDriver>, metric: &str, values: Vec<f64>) {
    if values.len() < 2 {
        return;
    }
    let after = *values.last().unwrap_or(&0.0);
    let baseline_slice = &values[..values.len() - 1];
    let before = baseline_slice.iter().sum::<f64>() / baseline_slice.len() as f64;
    let delta = after - before;
    if delta.abs() < f64::EPSILON {
        return;
    }
    drivers.push(AnomalyDriver {
        metric: metric.to_owned(),
        before,
        after,
        delta,
        summary: format!(
            "{} shifted from {} to {} (delta {}).",
            metric,
            format_metric_value(metric, before),
            format_metric_value(metric, after),
            format_metric_value(metric, delta)
        ),
    });
}

pub(crate) fn build_support_bundle_manifest(
    privacy_tier: ExportPrivacyTier,
    snapshot: SystemSnapshot,
    runtime_lag: RuntimeLagMetrics,
    diagnostics: DiagnosticsOverview,
    history: HistorySummaryResponse,
    diagnostic_events: Vec<DiagnosticsEvent>,
) -> Result<Vec<SupportBundleSectionManifest>, Value> {
    let sections = vec![
        section_manifest(
            "current_snapshot",
            "Live snapshot of host, capabilities, and current entities.",
            &snapshot,
            privacy_tier,
        )?,
        section_manifest(
            "runtime_lag",
            "Self-observability metrics for Aetower itself.",
            &runtime_lag,
            privacy_tier,
        )?,
        section_manifest(
            "diagnostics_overview",
            "Diagnostics ring and persistence overview.",
            &diagnostics,
            privacy_tier,
        )?,
        section_manifest(
            "history_summary",
            "Persisted history store range summary.",
            &history,
            privacy_tier,
        )?,
        section_manifest(
            "diagnostics_events",
            "Recent diagnostics events included in the support bundle payload.",
            &diagnostic_events,
            privacy_tier,
        )?,
    ];
    Ok(sections)
}

pub(crate) fn build_diagnostics_summary_report(
    overview: DiagnosticsOverview,
    events: Vec<DiagnosticsEvent>,
    options: DiagnosticsSummaryOptions,
) -> DiagnosticsSummaryReport {
    let event_count = events.len();
    let mut groups = BTreeMap::<(String, String, String), DiagnosticsSummaryGroup>::new();

    for event in events {
        let subsystem_label = diagnostics_subsystem_label(&event.subsystem);
        let level_label = diagnostics_level_label(&event.level);
        let key = (
            subsystem_label.to_owned(),
            event.event_type.clone(),
            level_label.to_owned(),
        );
        let detail = diagnostic_field_value(&event, "detail");
        let sample_fields = diagnostic_sample_fields(&event);
        groups
            .entry(key)
            .and_modify(|group| {
                group.count = group.count.saturating_add(1);
                group.first_millis = group.first_millis.min(event.timestamp_millis);
                if event.timestamp_millis >= group.latest_millis {
                    group.latest_millis = event.timestamp_millis;
                    group.latest_message = event.message.clone();
                    group.latest_detail = detail.clone();
                    group.sample_fields = sample_fields.clone();
                }
            })
            .or_insert_with(|| DiagnosticsSummaryGroup {
                subsystem: subsystem_label.to_owned(),
                event_type: event.event_type,
                level: level_label.to_owned(),
                count: 1,
                first_millis: event.timestamp_millis,
                latest_millis: event.timestamp_millis,
                latest_message: event.message,
                latest_detail: detail,
                sample_fields,
            });
    }

    let mut groups = groups.into_values().collect::<Vec<_>>();
    groups.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| right.latest_millis.cmp(&left.latest_millis))
            .then_with(|| left.event_type.cmp(&right.event_type))
    });
    groups.truncate(options.limit.max(1));
    let recommendations = diagnostics_summary_recommendations(&overview, event_count, &groups);

    DiagnosticsSummaryReport {
        overview,
        event_count,
        limit: options.limit.max(1),
        since_millis: options.since_millis,
        include_persisted: options.include_persisted,
        minimum_level: options.minimum_level,
        subsystem: options.subsystem,
        search: options.search,
        groups,
        recommendations,
    }
}

pub(crate) fn diagnostics_summary_recommendations(
    overview: &DiagnosticsOverview,
    event_count: usize,
    groups: &[DiagnosticsSummaryGroup],
) -> Vec<String> {
    let mut recommendations = Vec::new();
    if overview.error_count >= DIAGNOSTICS_ERROR_WARNING {
        recommendations.push(format!(
            "{} retained diagnostics errors are present; start with the top error-level group before paging raw rows.",
            overview.error_count
        ));
    }
    if overview.warn_count >= DIAGNOSTICS_WARN_WARNING {
        recommendations.push(format!(
            "{} retained diagnostics warnings are present; use the grouped summary to separate repeated noise from new failures.",
            overview.warn_count
        ));
    }
    if let Some(group) = groups.first()
        && group.count.saturating_mul(2) >= event_count.max(1)
    {
        recommendations.push(format!(
            "{}:{} dominates the sampled diagnostics with {} of {} event(s).",
            group.subsystem, group.event_type, group.count, event_count
        ));
    }
    if groups
        .iter()
        .any(|group| group.event_type == "host-incident-snapshot")
    {
        recommendations.push(
            "Host incident snapshots are present; correlate them with host alerts and top external burden leaders before blaming Aetower."
                .to_owned(),
        );
    }
    if groups.iter().any(|group| {
        group.event_type == "mcp-helper-reaped" || group.event_type == "mcp-helper-lifecycle"
    }) {
        recommendations.push(
            "MCP helper lifecycle diagnostics appeared; verify clients disconnect cleanly and watch helper count plus oldest-helper age in session health."
                .to_owned(),
        );
    }
    if recommendations.is_empty() {
        recommendations.push(
            "Diagnostics groups are currently low-signal; query raw diagnostics only for the event types that matter."
                .to_owned(),
        );
    }
    recommendations
}

pub(crate) fn diagnostics_level_label(level: &DiagnosticsLevel) -> &'static str {
    match level {
        DiagnosticsLevel::Trace => "trace",
        DiagnosticsLevel::Debug => "debug",
        DiagnosticsLevel::Info => "info",
        DiagnosticsLevel::Warn => "warn",
        DiagnosticsLevel::Error => "error",
    }
}

pub(crate) fn diagnostics_subsystem_label(subsystem: &DiagnosticsSubsystem) -> &'static str {
    match subsystem {
        DiagnosticsSubsystem::Engine => "engine",
        DiagnosticsSubsystem::Collector => "collector",
        DiagnosticsSubsystem::Identity => "identity",
        DiagnosticsSubsystem::Attribution => "attribution",
        DiagnosticsSubsystem::Friction => "friction",
        DiagnosticsSubsystem::History => "history",
        DiagnosticsSubsystem::Persistence => "persistence",
        DiagnosticsSubsystem::Telemetry => "telemetry",
        DiagnosticsSubsystem::Gpu => "gpu",
        DiagnosticsSubsystem::Ffi => "ffi",
        DiagnosticsSubsystem::Ui => "ui",
        DiagnosticsSubsystem::AdapterChromium => "adapter-chromium",
        DiagnosticsSubsystem::AdapterDocker => "adapter-docker",
        DiagnosticsSubsystem::AdapterHelper => "adapter-helper",
        DiagnosticsSubsystem::AdapterChau7 => "adapter-chau7",
        DiagnosticsSubsystem::AdapterVsCode => "adapter-vs-code",
    }
}

pub(crate) fn diagnostic_field_value(event: &DiagnosticsEvent, key: &str) -> Option<String> {
    event
        .fields
        .iter()
        .find(|field| field.key == key)
        .map(|field| field.value.clone())
}

pub(crate) fn diagnostic_sample_fields(event: &DiagnosticsEvent) -> BTreeMap<String, String> {
    event
        .fields
        .iter()
        .take(8)
        .map(|field| (field.key.clone(), field.value.clone()))
        .collect()
}

pub(crate) fn host_memory_pressure_guidance(
    snapshot: &SystemSnapshot,
    runtime: &RuntimeLagMetrics,
) -> String {
    let external = top_external_memory_entities(snapshot, 4);
    let external_labels = format_entity_burden_labels(&external, |entity| {
        format_bytes(entity.metrics.memory_resident_bytes)
    });
    let observer_context = format!(
        "Aetower self is CPU {:.1}%, memory {}, wakeups {:.0}/s.",
        runtime.self_cpu_percent,
        format_bytes(runtime.self_memory_bytes),
        runtime.self_wakeups_per_second
    );
    if external_labels.is_empty() {
        format!(
            "Compression ({}) and swap ({}) are elevated. No non-Aetower memory leader is visible in the current snapshot; inspect host-level pressure and only then Aetower self. {}",
            format_bytes(snapshot.host.compressed_memory_bytes),
            format_bytes(snapshot.host.swap_used_bytes),
            observer_context
        )
    } else {
        format!(
            "Compression ({}) and swap ({}) are elevated. Start with external memory leaders: {}. {}",
            format_bytes(snapshot.host.compressed_memory_bytes),
            format_bytes(snapshot.host.swap_used_bytes),
            external_labels,
            observer_context
        )
    }
}

pub(crate) fn host_wakeup_guidance(
    snapshot: &SystemSnapshot,
    runtime: &RuntimeLagMetrics,
) -> String {
    let external = top_external_wakeup_entities(snapshot, 4);
    let external_labels = format_entity_burden_labels(&external, |entity| {
        format!("{:.0}/s", entity.metrics.wakeups_per_second)
    });
    let observer_context = format!(
        "Aetower self wakeups are {:.0}/s with MCP {:.1} req/s.",
        runtime.self_wakeups_per_second, runtime.mcp_requests_per_second
    );
    if external_labels.is_empty() {
        format!(
            "Host wakeups are {:.0}/s. No non-Aetower wakeup leader is visible; inspect system services and Aetower self telemetry next. {}",
            snapshot.host.wakeups_per_second, observer_context
        )
    } else {
        format!(
            "Host wakeups are {:.0}/s. Start with external wakeup leaders: {}. {}",
            snapshot.host.wakeups_per_second, external_labels, observer_context
        )
    }
}

pub(crate) fn host_load_detail(snapshot: &SystemSnapshot) -> String {
    let sorted_entities = top_entities(snapshot, snapshot.entities.len().max(1));
    let external = sorted_entities
        .iter()
        .copied()
        .filter(|entity| !is_aetower_entity(entity))
        .take(3)
        .collect::<Vec<_>>();
    if external.is_empty() {
        let fallback = sorted_entities
            .into_iter()
            .take(3)
            .map(|entity| entity.display_name.clone())
            .collect::<Vec<_>>()
            .join(", ");
        if fallback.is_empty() {
            "No current burden leaders are visible.".to_owned()
        } else {
            format!("No non-Aetower burden leader is visible. Current leaders: {fallback}.")
        }
    } else {
        format!(
            "Top non-Aetower burden leaders: {}.",
            external
                .into_iter()
                .map(|entity| entity.display_name.clone())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

pub(crate) fn build_recommendations(
    snapshot: &SystemSnapshot,
    diagnostics: &DiagnosticsOverview,
    runtime: &RuntimeLagMetrics,
    history: &HistorySummaryResponse,
    limit: usize,
) -> Vec<RecommendationItem> {
    let mut recommendations = Vec::new();
    if memory_pressure_finding(snapshot).is_some() {
        recommendations.push(RecommendationItem {
            severity: SeverityBand::Critical,
            title: "Reduce current memory pressure".to_owned(),
            detail: host_memory_pressure_guidance(snapshot, runtime),
            entity_id: top_external_memory_entities(snapshot, 1)
                .into_iter()
                .next()
                .or_else(|| top_memory_entities(snapshot, 1).into_iter().next())
                .map(|entity| entity.entity_id.clone()),
            source: "host".to_owned(),
            expected_benefit: "Lower swap, better responsiveness, and less battery drain."
                .to_owned(),
            suggested_action: None,
            target_pid: None,
            target_label: None,
        });
    }
    if wakeup_finding(snapshot).is_some() {
        recommendations.push(RecommendationItem {
            severity: SeverityBand::Warning,
            title: "Reduce wakeup-heavy workloads".to_owned(),
            detail: host_wakeup_guidance(snapshot, runtime),
            entity_id: top_external_wakeup_entities(snapshot, 1)
                .into_iter()
                .next()
                .or_else(|| top_wakeup_entities(snapshot, 1).into_iter().next())
                .map(|entity| entity.entity_id.clone()),
            source: "host".to_owned(),
            expected_benefit: "Lower battery drain and smoother foreground interactivity."
                .to_owned(),
            suggested_action: None,
            target_pid: None,
            target_label: None,
        });
    }
    if history_store_finding(history).is_some() {
        recommendations.push(RecommendationItem {
            severity: history_store_severity(history),
            title: "Keep the persisted history store under control".to_owned(),
            detail: format!(
                "The history store is {} with {} WAL and {} quarantined rows.",
                format_bytes(history.store_bytes),
                format_bytes(history.wal_bytes),
                history.quarantine_count
            ),
            entity_id: None,
            source: "history".to_owned(),
            expected_benefit: "Faster History loads and healthier long-run storage behavior."
                .to_owned(),
            suggested_action: None,
            target_pid: None,
            target_label: None,
        });
    }
    if diagnostics_finding(diagnostics).is_some() {
        recommendations.push(RecommendationItem {
            severity: diagnostics_severity(diagnostics),
            title: "Reduce diagnostics churn".to_owned(),
            detail: format!(
                "{} warnings and {} errors are currently retained in the diagnostics ring.",
                diagnostics.warn_count, diagnostics.error_count
            ),
            entity_id: None,
            source: "diagnostics".to_owned(),
            expected_benefit: "Cleaner operator signal and less noisy support output.".to_owned(),
            suggested_action: None,
            target_pid: None,
            target_label: None,
        });
    }
    if mcp_helper_severity(runtime) != SeverityBand::Info {
        recommendations.push(RecommendationItem {
            severity: mcp_helper_severity(runtime),
            title: "Clean up stale MCP helper processes".to_owned(),
            detail: format!(
                "{} helper processes are currently visible, {} of them older than {} minutes.",
                runtime.mcp_helper_count,
                runtime.stale_mcp_helper_count,
                MCP_HELPER_STALE_MILLIS / 60_000
            ),
            entity_id: None,
            source: "mcp".to_owned(),
            expected_benefit: "Lower idle overhead and more trustworthy local-MCP session health."
                .to_owned(),
            suggested_action: None,
            target_pid: None,
            target_label: None,
        });
    }
    if self_runtime_severity(runtime) != SeverityBand::Info {
        recommendations.push(RecommendationItem {
            severity: self_runtime_severity(runtime),
            title: "Reduce Aetower observer overhead".to_owned(),
            detail: self_runtime_recommendation_detail(runtime),
            entity_id: None,
            source: "aetower-self".to_owned(),
            expected_benefit:
                "Lower observer-induced load and more trustworthy performance investigations."
                    .to_owned(),
            suggested_action: None,
            target_pid: None,
            target_label: None,
        });
    }
    for entity in top_entities(snapshot, limit) {
        for recommendation in entity.recommendations.iter().take(2) {
            recommendations.push(RecommendationItem {
                severity: if entity.anomaly_detected {
                    SeverityBand::Warning
                } else {
                    SeverityBand::Info
                },
                title: recommendation.title.clone(),
                detail: recommendation.detail.clone(),
                entity_id: Some(entity.entity_id.clone()),
                source: entity.display_name.clone(),
                expected_benefit:
                    "Higher-confidence attribution and lower friction in the affected group."
                        .to_owned(),
                suggested_action: recommendation.suggested_action.clone(),
                target_pid: recommendation.target_pid,
                target_label: recommendation.target_label.clone(),
            });
        }
    }
    recommendations.sort_by(|left, right| {
        right
            .severity
            .score()
            .cmp(&left.severity.score())
            .then_with(|| left.title.cmp(&right.title))
    });
    recommendations.truncate(limit.max(1));
    recommendations
}
