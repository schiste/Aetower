//! Snapshot-domain report builders for the MCP server.
//!
//! Each function takes a `SystemSnapshot` (plus the relevant ancillary data)
//! and produces the structured report that the corresponding `tool_*` method
//! returns to clients. Helpers and shared types live in the crate root.

use std::collections::BTreeMap;

use aetower_diagnostics::DiagnosticsOverview;
use aetower_model::SystemSnapshot;
use serde_json::json;

use crate::{
    COMPRESSED_MEMORY_CRITICAL_BYTES, COMPRESSED_MEMORY_WARNING_BYTES, CapabilityStatusItem,
    HistorySummaryResponse, HostAlert, MEMORY_PRESSURE_CRITICAL_RATIO,
    MEMORY_PRESSURE_WARNING_RATIO, RecentChangeItem, SWAP_CRITICAL_BYTES, SWAP_WARNING_BYTES,
    SeverityBand, TopFinding, WAKEUPS_CRITICAL, WAKEUPS_WARNING, capability_action_label,
    capability_operator_label, capability_severity, diagnostics_finding, format_bytes,
    format_entity_burden_labels, history_store_finding, is_aetower_entity, memory_pressure_finding,
    top_entities, top_external_memory_entities, top_external_wakeup_entities, top_memory_entities,
    top_wakeup_entities, wakeup_finding,
};

pub(crate) fn build_top_findings(
    snapshot: &SystemSnapshot,
    diagnostics: &DiagnosticsOverview,
    history: &HistorySummaryResponse,
    limit: usize,
) -> Vec<TopFinding> {
    let mut findings = Vec::new();
    if let Some(memory) = memory_pressure_finding(snapshot) {
        findings.push(memory);
    }
    if let Some(wakeups) = wakeup_finding(snapshot) {
        findings.push(wakeups);
    }
    if let Some(history_finding) = history_store_finding(history) {
        findings.push(history_finding);
    }
    if let Some(diagnostics_finding) = diagnostics_finding(diagnostics) {
        findings.push(diagnostics_finding);
    }

    for entity in top_entities(snapshot, 4) {
        findings.push(TopFinding {
            id: format!("entity:{}", entity.entity_id),
            severity: if entity.friction.total_score >= 20.0 {
                SeverityBand::Critical
            } else {
                SeverityBand::Warning
            },
            title: format!("{} is a top current friction source", entity.display_name),
            detail: format!(
                "{:.1}% CPU, {} resident, friction {:.1}. {}",
                entity.metrics.cpu_percent,
                format_bytes(entity.metrics.memory_resident_bytes),
                entity.friction.total_score,
                entity
                    .recent_change_summary
                    .clone()
                    .unwrap_or_else(|| "No recent change summary is attached.".to_owned())
            ),
            source: "entity".to_owned(),
            entity_ids: vec![entity.entity_id.clone()],
            recommendation: entity.recommendations.first().map(|recommendation| {
                format!("{}: {}", recommendation.title, recommendation.detail)
            }),
        });
    }

    findings.sort_by(|left, right| {
        right
            .severity
            .score()
            .cmp(&left.severity.score())
            .then_with(|| left.id.cmp(&right.id))
    });
    findings.truncate(limit.max(1));
    findings
}

pub(crate) fn build_host_alerts(
    snapshot: &SystemSnapshot,
    top_entity_limit: usize,
) -> Vec<HostAlert> {
    let mut alerts = Vec::new();
    let used_ratio = if snapshot.host.memory_total_bytes == 0 {
        0.0
    } else {
        snapshot.host.memory_used_bytes as f64 / snapshot.host.memory_total_bytes as f64
    };
    let top_memory_entities = top_memory_entities(snapshot, top_entity_limit);
    let top_external_memory = top_external_memory_entities(snapshot, top_entity_limit);
    let top_memory_entity_labels = if top_external_memory.is_empty() {
        format_entity_burden_labels(&top_memory_entities, |entity| {
            format_bytes(entity.metrics.memory_resident_bytes)
        })
    } else {
        format_entity_burden_labels(&top_external_memory, |entity| {
            format_bytes(entity.metrics.memory_resident_bytes)
        })
    };
    if used_ratio >= MEMORY_PRESSURE_WARNING_RATIO
        || snapshot.host.compressed_memory_bytes >= COMPRESSED_MEMORY_WARNING_BYTES
        || snapshot.host.swap_used_bytes >= SWAP_WARNING_BYTES
    {
        let severity = if used_ratio >= MEMORY_PRESSURE_CRITICAL_RATIO
            || snapshot.host.compressed_memory_bytes >= COMPRESSED_MEMORY_CRITICAL_BYTES
            || snapshot.host.swap_used_bytes >= SWAP_CRITICAL_BYTES
        {
            SeverityBand::Critical
        } else {
            SeverityBand::Warning
        };
        let mut metrics = BTreeMap::new();
        metrics.insert(
            "memory_used_bytes".to_owned(),
            json!(snapshot.host.memory_used_bytes),
        );
        metrics.insert(
            "memory_total_bytes".to_owned(),
            json!(snapshot.host.memory_total_bytes),
        );
        metrics.insert(
            "compressed_memory_bytes".to_owned(),
            json!(snapshot.host.compressed_memory_bytes),
        );
        metrics.insert(
            "swap_used_bytes".to_owned(),
            json!(snapshot.host.swap_used_bytes),
        );
        alerts.push(HostAlert {
            id: "host-memory-pressure".to_owned(),
            severity,
            category: "memory-pressure".to_owned(),
            title: "Host memory pressure is elevated".to_owned(),
            detail: format!(
                "{} used of {}, {} compressed, {} swap. Top current groups: {}.",
                format_bytes(snapshot.host.memory_used_bytes),
                format_bytes(snapshot.host.memory_total_bytes),
                format_bytes(snapshot.host.compressed_memory_bytes),
                format_bytes(snapshot.host.swap_used_bytes),
                if top_memory_entity_labels.is_empty() {
                    "none".to_owned()
                } else if top_external_memory.is_empty() {
                    format!(
                        "no non-Aetower leader visible; current leaders {top_memory_entity_labels}"
                    )
                } else {
                    format!("external leaders {top_memory_entity_labels}")
                }
            ),
            metrics,
            entity_ids: if top_external_memory.is_empty() {
                top_memory_entities
            } else {
                top_external_memory
            }
            .into_iter()
            .map(|entity| entity.entity_id.clone())
            .collect(),
        });
    }

    if snapshot.host.wakeups_per_second >= WAKEUPS_WARNING {
        let severity = if snapshot.host.wakeups_per_second >= WAKEUPS_CRITICAL {
            SeverityBand::Critical
        } else {
            SeverityBand::Warning
        };
        let external_wakeup_leaders = top_external_wakeup_entities(snapshot, top_entity_limit);
        let wakeup_leaders = top_wakeup_entities(snapshot, top_entity_limit);
        let leader = external_wakeup_leaders
            .first()
            .copied()
            .or_else(|| wakeup_leaders.first().copied());
        let mut metrics = BTreeMap::new();
        metrics.insert(
            "host_wakeups_per_second".to_owned(),
            json!(snapshot.host.wakeups_per_second),
        );
        if let Some(leader) = leader {
            metrics.insert(
                "leader_wakeups_per_second".to_owned(),
                json!(leader.metrics.wakeups_per_second),
            );
        }
        alerts.push(HostAlert {
            id: "host-wakeup-storm".to_owned(),
            severity,
            category: "wakeups".to_owned(),
            title: "Wakeup rate is high".to_owned(),
            detail: leader.map_or_else(
                || format!(
                    "Host wakeups are {:.0}/s with no single entity leader identified.",
                    snapshot.host.wakeups_per_second
                ),
                |leader| {
                    if is_aetower_entity(leader) {
                        format!(
                            "Host wakeups are {:.0}/s. No non-Aetower wakeup leader is visible; Aetower leads at {:.0}/s, so check self telemetry and MCP request rate.",
                            snapshot.host.wakeups_per_second,
                            leader.metrics.wakeups_per_second
                        )
                    } else {
                        format!(
                            "Host wakeups are {:.0}/s. External leader {} is at {:.0}/s.",
                            snapshot.host.wakeups_per_second,
                            leader.display_name,
                            leader.metrics.wakeups_per_second
                        )
                    }
                },
            ),
            metrics,
            entity_ids: leader
                .map(|entity| vec![entity.entity_id.clone()])
                .unwrap_or_default(),
        });
    }

    alerts
}

pub(crate) fn build_recent_changes(
    snapshot: &SystemSnapshot,
    window_millis: u64,
    limit: usize,
) -> Vec<RecentChangeItem> {
    let since = snapshot.captured_at_millis.saturating_sub(window_millis);
    let mut changes = snapshot
        .timeline
        .iter()
        .filter(|event| event.timestamp_millis >= since)
        .map(|event| RecentChangeItem {
            timestamp_millis: event.timestamp_millis,
            severity: match event.severity {
                aetower_model::TimelineSeverity::Info => SeverityBand::Info,
                aetower_model::TimelineSeverity::Warning => SeverityBand::Warning,
                aetower_model::TimelineSeverity::Critical => SeverityBand::Critical,
            },
            source: format!("timeline:{:?}", event.category).to_lowercase(),
            entity_id: event.entity_id.clone(),
            title: event.title.clone(),
            detail: event.detail.clone(),
        })
        .collect::<Vec<_>>();

    for entity in &snapshot.entities {
        if let Some(summary) = &entity.recent_change_summary {
            changes.push(RecentChangeItem {
                timestamp_millis: snapshot.captured_at_millis,
                severity: if entity.anomaly_detected {
                    SeverityBand::Warning
                } else {
                    SeverityBand::Info
                },
                source: "entity-summary".to_owned(),
                entity_id: Some(entity.entity_id.clone()),
                title: format!("{} changed recently", entity.display_name),
                detail: summary.clone(),
            });
        }
    }

    changes.sort_by(|left, right| {
        right
            .timestamp_millis
            .cmp(&left.timestamp_millis)
            .then_with(|| right.severity.score().cmp(&left.severity.score()))
    });
    changes.truncate(limit.max(1));
    changes
}

pub(crate) fn build_capability_status(snapshot: &SystemSnapshot) -> Vec<CapabilityStatusItem> {
    let mut capabilities = snapshot
        .capabilities
        .iter()
        .map(|capability| CapabilityStatusItem {
            kind: format!("{:?}", capability.kind),
            state: format!("{:?}", capability.state),
            health: format!("{:?}", capability.health),
            operator_label: capability_operator_label(capability),
            action_label: capability_action_label(capability),
            detail: capability.detail.clone(),
            last_updated_millis: capability.last_updated_millis,
            severity: capability_severity(capability),
        })
        .collect::<Vec<_>>();
    capabilities.sort_by(|left, right| {
        right
            .severity
            .score()
            .cmp(&left.severity.score())
            .then_with(|| left.kind.cmp(&right.kind))
    });
    capabilities
}
