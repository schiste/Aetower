//! History-domain report builders for the MCP server.

use std::collections::BTreeMap;

use aetower_diagnostics::DiagnosticsEvent;
use aetower_model::SystemSnapshot;
use serde_json::json;

use crate::*;

pub(crate) fn build_snapshot_diff_report(
    before: &SystemSnapshot,
    after: &SystemSnapshot,
    requested_entity_ids: &[String],
    limit: usize,
) -> SnapshotDiffReport {
    build_snapshot_diff_report_with_diagnostics(before, after, requested_entity_ids, limit, &[])
}

pub(crate) fn build_snapshot_diff_report_with_diagnostics(
    before: &SystemSnapshot,
    after: &SystemSnapshot,
    requested_entity_ids: &[String],
    limit: usize,
    diagnostics: &[DiagnosticsEvent],
) -> SnapshotDiffReport {
    let before_boot = before.host.boot_session.as_ref();
    let after_boot = after.host.boot_session.as_ref();
    let before_boot_id = before_boot.and_then(|boot| boot.boot_id.clone());
    let after_boot_id = after_boot.and_then(|boot| boot.boot_id.clone());
    let before_boot_time_millis = before_boot.and_then(|boot| boot.boot_time_millis);
    let after_boot_time_millis = after_boot.and_then(|boot| boot.boot_time_millis);
    let crossed_boot_boundary =
        before_boot_id != after_boot_id || before_boot_time_millis != after_boot_time_millis;
    let mut host = BTreeMap::new();
    host.insert(
        "cpu_percent".to_owned(),
        metric_delta(
            before.host.cpu_percent as f64,
            after.host.cpu_percent as f64,
        ),
    );
    host.insert(
        "memory_used_bytes".to_owned(),
        metric_delta(
            before.host.memory_used_bytes as f64,
            after.host.memory_used_bytes as f64,
        ),
    );
    host.insert(
        "compressed_memory_bytes".to_owned(),
        metric_delta(
            before.host.compressed_memory_bytes as f64,
            after.host.compressed_memory_bytes as f64,
        ),
    );
    host.insert(
        "swap_used_bytes".to_owned(),
        metric_delta(
            before.host.swap_used_bytes as f64,
            after.host.swap_used_bytes as f64,
        ),
    );
    host.insert(
        "wakeups_per_second".to_owned(),
        metric_delta(
            before.host.wakeups_per_second as f64,
            after.host.wakeups_per_second as f64,
        ),
    );

    let before_entities = before
        .entities
        .iter()
        .map(|entity| (entity.entity_id.clone(), entity))
        .collect::<BTreeMap<_, _>>();
    let after_entities = after
        .entities
        .iter()
        .map(|entity| (entity.entity_id.clone(), entity))
        .collect::<BTreeMap<_, _>>();
    let entity_ids = if requested_entity_ids.is_empty() {
        before_entities
            .keys()
            .chain(after_entities.keys())
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
    } else {
        requested_entity_ids.to_vec()
    };
    let mut entities = entity_ids
        .into_iter()
        .filter_map(|entity_id| {
            let before_entity = before_entities.get(&entity_id).copied();
            let after_entity = after_entities.get(&entity_id).copied();
            let display_name = after_entity
                .or(before_entity)
                .map(|entity| entity.display_name.clone())?;
            let before_metrics = before_entity.map(|entity| &entity.metrics);
            let after_metrics = after_entity.map(|entity| &entity.metrics);
            Some(SnapshotEntityDelta {
                entity_id: entity_id.clone(),
                display_name,
                before_present: before_entity.is_some(),
                after_present: after_entity.is_some(),
                friction: metric_delta(
                    before_entity
                        .map(|entity| entity.friction.total_score as f64)
                        .unwrap_or(0.0),
                    after_entity
                        .map(|entity| entity.friction.total_score as f64)
                        .unwrap_or(0.0),
                ),
                cpu_percent: metric_delta(
                    before_metrics
                        .map(|metrics| metrics.cpu_percent as f64)
                        .unwrap_or(0.0),
                    after_metrics
                        .map(|metrics| metrics.cpu_percent as f64)
                        .unwrap_or(0.0),
                ),
                memory_bytes: metric_delta(
                    before_metrics
                        .map(|metrics| metrics.memory_resident_bytes as f64)
                        .unwrap_or(0.0),
                    after_metrics
                        .map(|metrics| metrics.memory_resident_bytes as f64)
                        .unwrap_or(0.0),
                ),
                memory_physical_footprint_bytes: metric_delta(
                    before_metrics
                        .map(|metrics| metrics.memory_physical_footprint_bytes)
                        .map(|value| value as f64)
                        .unwrap_or(0.0),
                    after_metrics
                        .map(|metrics| metrics.memory_physical_footprint_bytes)
                        .map(|value| value as f64)
                        .unwrap_or(0.0),
                ),
                wakeups_per_second: metric_delta(
                    before_metrics
                        .map(|metrics| metrics.wakeups_per_second as f64)
                        .unwrap_or(0.0),
                    after_metrics
                        .map(|metrics| metrics.wakeups_per_second as f64)
                        .unwrap_or(0.0),
                ),
                process_count: metric_delta(
                    before_metrics
                        .map(|metrics| metrics.process_count as f64)
                        .unwrap_or(0.0),
                    after_metrics
                        .map(|metrics| metrics.process_count as f64)
                        .unwrap_or(0.0),
                ),
                recent_change_summary: after_entity
                    .and_then(|entity| entity.recent_change_summary.clone())
                    .or_else(|| {
                        before_entity.and_then(|entity| entity.recent_change_summary.clone())
                    }),
            })
        })
        .collect::<Vec<_>>();
    entities.sort_by(|left, right| {
        right
            .friction
            .delta
            .abs()
            .partial_cmp(&left.friction.delta.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                right
                    .wakeups_per_second
                    .delta
                    .abs()
                    .partial_cmp(&left.wakeups_per_second.delta.abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    if requested_entity_ids.is_empty() {
        entities.truncate(limit.max(1));
    }

    let cpu_delta = host
        .get("cpu_percent")
        .map(|metric| format_signed_decimal(metric.delta, "%"))
        .unwrap_or_else(|| "n/a".to_owned());
    let memory_delta = host
        .get("memory_used_bytes")
        .map(|metric| format_signed_bytes(metric.delta))
        .unwrap_or_else(|| "n/a".to_owned());
    let wakeup_delta = host
        .get("wakeups_per_second")
        .map(|metric| format_signed_rate(metric.delta))
        .unwrap_or_else(|| "n/a".to_owned());
    let host_summary = if crossed_boot_boundary {
        format!(
            "Crossed a reboot boundary. CPU {}, memory {}, wakeups {} across the selected window.",
            cpu_delta, memory_delta, wakeup_delta
        )
    } else {
        format!(
            "CPU {}, memory {}, wakeups {} across the selected window.",
            cpu_delta, memory_delta, wakeup_delta
        )
    };

    let appeared = entities
        .iter()
        .filter(|entity| !entity.before_present && entity.after_present)
        .count();
    let disappeared = entities
        .iter()
        .filter(|entity| entity.before_present && !entity.after_present)
        .count();
    let changed = entities
        .iter()
        .filter(|entity| entity.before_present && entity.after_present)
        .count();
    let entity_summary = if let Some(top_entity) = entities.first() {
        format!(
            "{} changed, {} appeared, {} disappeared. Largest shift: {} (friction {}).",
            changed,
            appeared,
            disappeared,
            top_entity.display_name,
            format_signed_decimal(top_entity.friction.delta, "")
        )
    } else {
        "No entity deltas were available for the selected window.".to_owned()
    };

    SnapshotDiffReport {
        before_snapshot_millis: before.captured_at_millis,
        after_snapshot_millis: after.captured_at_millis,
        crossed_boot_boundary,
        before_boot_id,
        after_boot_id,
        before_boot_time_millis,
        after_boot_time_millis,
        before_previous_shutdown: before_boot
            .and_then(|boot| boot.previous_shutdown.clone())
            .or_else(|| {
                extract_previous_shutdown_from_events(
                    diagnostics,
                    before.captured_at_millis.saturating_sub(30 * 60 * 1000),
                    before.captured_at_millis.saturating_add(5 * 60 * 1000),
                )
            }),
        after_previous_shutdown: after_boot
            .and_then(|boot| boot.previous_shutdown.clone())
            .or_else(|| {
                extract_previous_shutdown_from_events(
                    diagnostics,
                    after.captured_at_millis.saturating_sub(30 * 60 * 1000),
                    after.captured_at_millis.saturating_add(5 * 60 * 1000),
                )
            }),
        summary: SnapshotDiffSummary {
            host_summary,
            entity_summary,
        },
        host,
        entities,
    }
}

fn format_signed_decimal(value: f64, suffix: &str) -> String {
    let formatted = format!("{:.1}{suffix}", value.abs());
    if value > 0.0 {
        format!("+{formatted}")
    } else if value < 0.0 {
        format!("-{formatted}")
    } else {
        formatted
    }
}

fn format_signed_bytes(value: f64) -> String {
    let bytes = value.abs().round() as u64;
    let formatted = format_bytes(bytes);
    if value > 0.0 {
        format!("+{formatted}")
    } else if value < 0.0 {
        format!("-{formatted}")
    } else {
        formatted
    }
}

fn format_signed_rate(value: f64) -> String {
    let formatted = format!("{:.0}/s", value.abs());
    if value > 0.0 {
        format!("+{formatted}")
    } else if value < 0.0 {
        format!("-{formatted}")
    } else {
        formatted
    }
}

pub(crate) fn build_reboot_report(
    data_source: &dyn AetowerMcpDataSource,
    start_millis: u64,
    end_millis: u64,
) -> Result<RebootReport, String> {
    let snapshots = load_history_snapshots(data_source, start_millis, end_millis, 2048)?;
    if snapshots.is_empty() {
        return Err(
            "No persisted snapshots were available for that reboot report window.".to_owned(),
        );
    }
    let mut sessions = Vec::<Vec<SystemSnapshot>>::new();
    for snapshot in snapshots {
        let key = snapshot_boot_key(&snapshot.host);
        match sessions.last_mut() {
            Some(current)
                if current
                    .last()
                    .map(|last| snapshot_boot_key(&last.host) == key)
                    .unwrap_or(false) =>
            {
                current.push(snapshot);
            }
            _ => sessions.push(vec![snapshot]),
        }
    }

    let diagnostics = load_reboot_diagnostics(
        data_source,
        start_millis.saturating_sub(60 * 60 * 1000),
        end_millis.saturating_add(30 * 60 * 1000),
    )?;
    let session_reports = sessions
        .iter()
        .filter_map(|session| build_reboot_session_report(session))
        .collect::<Vec<_>>();
    let mut boundaries = Vec::new();
    for pair in sessions.windows(2) {
        let before = pair.first().and_then(|session| session.last());
        let after = pair.get(1).and_then(|session| session.first());
        if let (Some(before), Some(after)) = (before, after) {
            boundaries.push(build_reboot_boundary_report(before, after, &diagnostics));
        }
    }

    Ok(RebootReport {
        range_start_millis: start_millis,
        range_end_millis: end_millis,
        session_count: session_reports.len(),
        boundary_count: boundaries.len(),
        sessions: session_reports,
        boundaries,
    })
}

pub fn diff_snapshots_json(
    data_source: &dyn AetowerMcpDataSource,
    before_millis: u64,
    after_millis: u64,
    entity_ids: &[String],
    limit: usize,
) -> Result<String, String> {
    let start_millis = before_millis.min(after_millis);
    let end_millis = before_millis.max(after_millis);
    let snapshots = data_source.load_history_page(start_millis, end_millis, None, 512)?;
    if snapshots.is_empty() {
        return Err("No persisted snapshots were available for that comparison window.".to_owned());
    }
    let before = snapshots
        .iter()
        .min_by_key(|snapshot| snapshot.captured_at_millis.abs_diff(before_millis))
        .ok_or_else(|| "Could not resolve the before snapshot.".to_owned())?;
    let after = snapshots
        .iter()
        .min_by_key(|snapshot| snapshot.captured_at_millis.abs_diff(after_millis))
        .ok_or_else(|| "Could not resolve the after snapshot.".to_owned())?;
    let diagnostics = load_reboot_diagnostics(
        data_source,
        start_millis.saturating_sub(60 * 60 * 1000),
        end_millis.saturating_add(30 * 60 * 1000),
    )?;
    let report =
        build_snapshot_diff_report_with_diagnostics(before, after, entity_ids, limit, &diagnostics);
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

fn load_reboot_diagnostics(
    data_source: &dyn AetowerMcpDataSource,
    start_millis: u64,
    end_millis: u64,
) -> Result<Vec<DiagnosticsEvent>, String> {
    const SEARCHES: &[(&str, usize)] = &[
        ("boot-session-observed", 256),
        ("system-previous-shutdown-cause", 128),
        ("system-panic-marker", 128),
        ("system-sleep-marker", 256),
        ("system-wake-marker", 256),
        ("system-thermal-marker", 256),
        ("system-power-marker", 256),
        ("engine-initialized", 128),
        ("host-incident-snapshot", 512),
        ("tick-over-budget", 2048),
    ];

    let mut events = Vec::new();
    for (search, limit) in SEARCHES {
        let mut chunk = data_source.query_diagnostics(DiagnosticsQuery {
            limit: *limit,
            search: Some((*search).to_owned()),
            since_millis: Some(start_millis),
            include_persisted: true,
            ..DiagnosticsQuery::default()
        })?;
        chunk.retain(|event| event.timestamp_millis <= end_millis);
        events.extend(chunk);
    }
    events.sort_by(|left, right| {
        left.timestamp_millis
            .cmp(&right.timestamp_millis)
            .then(left.id.cmp(&right.id))
    });
    events.dedup_by(|left, right| {
        if !left.id.is_empty() && !right.id.is_empty() {
            left.id == right.id
        } else {
            left.timestamp_millis == right.timestamp_millis
                && left.event_type == right.event_type
                && left.message == right.message
        }
    });
    Ok(events)
}

pub(crate) fn load_history_snapshots(
    data_source: &dyn AetowerMcpDataSource,
    start_millis: u64,
    end_millis: u64,
    max_snapshots: usize,
) -> Result<Vec<SystemSnapshot>, String> {
    let mut snapshots =
        load_history_snapshots_raw(data_source, start_millis, end_millis, max_snapshots)?;
    snapshots.dedup_by(|left, right| {
        left.captured_at_millis == right.captured_at_millis && left.sequence == right.sequence
    });
    Ok(snapshots)
}

pub(crate) fn load_history_snapshots_raw(
    data_source: &dyn AetowerMcpDataSource,
    start_millis: u64,
    end_millis: u64,
    max_snapshots: usize,
) -> Result<Vec<SystemSnapshot>, String> {
    let mut before_cursor = None;
    let mut snapshots = Vec::new();
    let page_limit = 256u32;
    while snapshots.len() < max_snapshots {
        let page =
            data_source.load_history_page(start_millis, end_millis, before_cursor, page_limit)?;
        if page.is_empty() {
            break;
        }
        let next_before_cursor = older_page_cursor(&page);
        if before_cursor.is_some() && next_before_cursor == before_cursor {
            break;
        }
        snapshots.extend(page);
        let Some(next_before_cursor) = next_before_cursor else {
            break;
        };
        before_cursor = Some(next_before_cursor);
        if snapshots.len() >= max_snapshots {
            break;
        }
    }
    snapshots.sort_by_key(|snapshot| snapshot.captured_at_millis);
    Ok(snapshots)
}

pub(crate) fn older_page_cursor(snapshots: &[SystemSnapshot]) -> Option<u64> {
    snapshots
        .iter()
        .map(|snapshot| snapshot.captured_at_millis)
        .min()
        .and_then(|captured_at| captured_at.checked_sub(1))
}

pub(crate) fn build_history_data_quality_report(
    snapshots: &[SystemSnapshot],
    window_start_millis: u64,
    window_end_millis: u64,
    expected_interval_millis: u64,
) -> HistoryDataQualityReport {
    let expected_interval_millis = expected_interval_millis.max(1);
    let gap_threshold_millis = expected_interval_millis.saturating_mul(HISTORY_DATA_GAP_MULTIPLIER);
    let sampled_snapshots = snapshots.len();
    let oldest_millis = snapshots
        .first()
        .map(|snapshot| snapshot.captured_at_millis);
    let newest_millis = snapshots.last().map(|snapshot| snapshot.captured_at_millis);
    let coverage_millis = newest_millis
        .zip(oldest_millis)
        .map(|(newest, oldest)| newest.saturating_sub(oldest))
        .unwrap_or(0);
    let requested_window_millis = window_end_millis.saturating_sub(window_start_millis);
    let coverage_ratio = if requested_window_millis == 0 {
        0.0
    } else {
        (coverage_millis as f64 / requested_window_millis as f64).clamp(0.0, 1.0)
    };

    let mut timestamp_counts = BTreeMap::<u64, usize>::new();
    for snapshot in snapshots {
        *timestamp_counts
            .entry(snapshot.captured_at_millis)
            .or_default() += 1;
    }
    let duplicate_timestamp_count = timestamp_counts
        .values()
        .map(|count| count.saturating_sub(1))
        .sum();

    let mut largest_gap_millis = 0u64;
    let mut gap_count = 0usize;
    let mut sequence_regression_count = 0usize;
    let mut sequence_reset_count = 0usize;
    let mut boot_boundary_count = 0usize;
    for pair in snapshots.windows(2) {
        let before = &pair[0];
        let after = &pair[1];
        let gap_millis = after
            .captured_at_millis
            .saturating_sub(before.captured_at_millis);
        largest_gap_millis = largest_gap_millis.max(gap_millis);
        if gap_millis > gap_threshold_millis {
            gap_count += 1;
        }
        if after.sequence < before.sequence {
            if gap_millis > gap_threshold_millis {
                sequence_reset_count += 1;
            } else {
                sequence_regression_count += 1;
            }
        }
        if snapshot_boot_key(&before.host) != snapshot_boot_key(&after.host) {
            boot_boundary_count += 1;
        }
    }

    let severity = if sampled_snapshots == 0
        || sequence_regression_count > 0
        || largest_gap_millis >= 60 * 60 * 1000
        || duplicate_timestamp_count >= 64
    {
        SeverityBand::Critical
    } else if sampled_snapshots < 2 || gap_count > 0 || duplicate_timestamp_count > 0 {
        SeverityBand::Warning
    } else {
        SeverityBand::Info
    };

    let mut recommendations = Vec::new();
    if sampled_snapshots == 0 {
        recommendations.push(
            "History has no readable snapshots in this window; verify writer startup, SQLite path, and quarantine diagnostics."
                .to_owned(),
        );
    }
    if gap_count > 0 {
        recommendations.push(format!(
            "Investigate {} history coverage gap(s) above {}.",
            gap_count,
            format_duration_millis(gap_threshold_millis)
        ));
    }
    if duplicate_timestamp_count > 0 {
        recommendations.push(
            "Audit history writer idempotency; duplicate captured_at timestamps should be rare."
                .to_owned(),
        );
    }
    if sequence_regression_count > 0 {
        recommendations.push(
            "Sequence numbers regressed inside a continuous history stream; audit writer ordering and flush concurrency."
                .to_owned(),
        );
    }
    if sequence_reset_count > 0 {
        recommendations.push(
            "Sequence numbers reset after a coverage gap, which usually indicates an app restart; correlate with engine diagnostics before treating it as corruption."
                .to_owned(),
        );
    }
    if recommendations.is_empty() {
        recommendations
            .push("History ordering and coverage look healthy for this window.".to_owned());
    }

    HistoryDataQualityReport {
        severity,
        summary: match severity {
            SeverityBand::Info => format!(
                "{} snapshots sampled; largest gap {}; no ordering issues detected.",
                sampled_snapshots,
                format_duration_millis(largest_gap_millis)
            ),
            SeverityBand::Warning => format!(
                "{} snapshots sampled; {} gap(s), {} duplicate timestamp(s), {} sequence reset(s), largest gap {}.",
                sampled_snapshots,
                gap_count,
                duplicate_timestamp_count,
                sequence_reset_count,
                format_duration_millis(largest_gap_millis)
            ),
            SeverityBand::Critical => format!(
                "{} snapshots sampled; {} in-stream sequence regression(s), {} gap(s), largest gap {}.",
                sampled_snapshots,
                sequence_regression_count,
                gap_count,
                format_duration_millis(largest_gap_millis)
            ),
        },
        window_start_millis,
        window_end_millis,
        sampled_snapshots,
        oldest_millis,
        newest_millis,
        coverage_millis,
        coverage_ratio,
        expected_interval_millis,
        gap_threshold_millis,
        largest_gap_millis,
        gap_count,
        duplicate_timestamp_count,
        sequence_regression_count,
        sequence_reset_count,
        boot_boundary_count,
        recommendations,
    }
}

pub(crate) fn select_investigation_entity_ids(
    snapshot: &SystemSnapshot,
    requested_entity_ids: &[String],
    host_alerts: &[HostAlert],
    limit: usize,
) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut selected = Vec::new();
    let mut push_id = |entity_id: &str| {
        if !entity_id.is_empty() && seen.insert(entity_id.to_owned()) {
            selected.push(entity_id.to_owned());
        }
    };

    if requested_entity_ids.is_empty() {
        for alert in host_alerts {
            for entity_id in &alert.entity_ids {
                push_id(entity_id);
            }
        }
        for entity in top_entities(snapshot, limit) {
            push_id(&entity.entity_id);
        }
    } else {
        for entity_id in requested_entity_ids {
            push_id(entity_id);
        }
    }

    selected.truncate(limit.max(1));
    selected
}

pub(crate) fn investigation_history_diff(
    latest: &SystemSnapshot,
    history_snapshots: &[SystemSnapshot],
    entity_ids: &[String],
    limit: usize,
    diagnostics: &[DiagnosticsEvent],
    caveats: &mut Vec<String>,
) -> Option<SnapshotDiffReport> {
    let Some(before) = history_snapshots.first() else {
        caveats.push(
            "No persisted snapshots were available inside the investigation window.".to_owned(),
        );
        return None;
    };
    let after = history_snapshots
        .last()
        .filter(|after| after.captured_at_millis >= before.captured_at_millis)
        .unwrap_or(latest);
    if before.captured_at_millis == after.captured_at_millis && before.sequence == after.sequence {
        caveats.push(
            "Only one persisted snapshot was available inside the investigation window; history diff omitted."
                .to_owned(),
        );
        return None;
    }
    Some(build_snapshot_diff_report_with_diagnostics(
        before,
        after,
        entity_ids,
        limit,
        diagnostics,
    ))
}

pub(crate) fn build_reboot_session_report(
    session: &[SystemSnapshot],
) -> Option<RebootSessionReport> {
    let first = session.first()?;
    let last = session.last()?;
    Some(RebootSessionReport {
        boot_id: first
            .host
            .boot_session
            .as_ref()
            .and_then(|boot| boot.boot_id.clone()),
        boot_time_millis: first
            .host
            .boot_session
            .as_ref()
            .and_then(|boot| boot.boot_time_millis),
        first_snapshot_millis: first.captured_at_millis,
        last_snapshot_millis: last.captured_at_millis,
        snapshot_count: session.len(),
        first_sequence: first.sequence,
        last_sequence: last.sequence,
        previous_shutdown: first
            .host
            .boot_session
            .as_ref()
            .and_then(|boot| boot.previous_shutdown.clone()),
    })
}

pub(crate) fn build_reboot_boundary_report(
    before: &SystemSnapshot,
    after: &SystemSnapshot,
    diagnostics: &[DiagnosticsEvent],
) -> RebootBoundaryReport {
    let reboot_detected_at_millis = after
        .host
        .boot_session
        .as_ref()
        .and_then(|boot| boot.boot_time_millis)
        .or(Some(after.captured_at_millis));
    let window_start = before.captured_at_millis.saturating_sub(30 * 60 * 1000);
    let window_end = after.captured_at_millis.saturating_add(30 * 60 * 1000);
    let correlated_markers = diagnostics
        .iter()
        .filter(|event| {
            event.timestamp_millis >= window_start
                && event.timestamp_millis <= window_end
                && matches!(
                    event.event_type.as_str(),
                    "boot-session-observed"
                        | "system-previous-shutdown-cause"
                        | "system-sleep-marker"
                        | "system-wake-marker"
                        | "system-panic-marker"
                        | "system-thermal-marker"
                        | "system-power-marker"
                        | "engine-initialized"
                )
        })
        .map(reboot_correlation_marker)
        .collect::<Vec<_>>();
    let pre_reboot_incidents = diagnostics
        .iter()
        .filter(|event| {
            event.timestamp_millis >= window_start
                && event.timestamp_millis <= before.captured_at_millis
                && matches!(
                    event.event_type.as_str(),
                    "host-incident-snapshot" | "tick-over-budget"
                )
        })
        .map(reboot_correlation_marker)
        .collect::<Vec<_>>();
    RebootBoundaryReport {
        before_snapshot_millis: before.captured_at_millis,
        after_snapshot_millis: after.captured_at_millis,
        reboot_detected_at_millis,
        gap_millis: after
            .captured_at_millis
            .saturating_sub(before.captured_at_millis),
        before_boot_id: before
            .host
            .boot_session
            .as_ref()
            .and_then(|boot| boot.boot_id.clone()),
        after_boot_id: after
            .host
            .boot_session
            .as_ref()
            .and_then(|boot| boot.boot_id.clone()),
        before_boot_time_millis: before
            .host
            .boot_session
            .as_ref()
            .and_then(|boot| boot.boot_time_millis),
        after_boot_time_millis: after
            .host
            .boot_session
            .as_ref()
            .and_then(|boot| boot.boot_time_millis),
        previous_shutdown: after
            .host
            .boot_session
            .as_ref()
            .and_then(|boot| boot.previous_shutdown.clone())
            .or_else(|| {
                extract_previous_shutdown_from_events(diagnostics, window_start, window_end)
            }),
        before_top_entities: before
            .entities
            .iter()
            .take(5)
            .map(|entity| entity.display_name.clone())
            .collect(),
        after_top_entities: after
            .entities
            .iter()
            .take(5)
            .map(|entity| entity.display_name.clone())
            .collect(),
        correlated_markers,
        pre_reboot_incidents,
    }
}

fn reboot_correlation_marker(event: &DiagnosticsEvent) -> RebootCorrelationMarker {
    RebootCorrelationMarker {
        timestamp_millis: event.timestamp_millis,
        event_type: event.event_type.clone(),
        level: format!("{:?}", event.level).to_ascii_lowercase(),
        message: event.message.clone(),
        detail: event
            .fields
            .iter()
            .find(|field| field.key == "detail")
            .map(|field| field.value.clone()),
    }
}

fn extract_previous_shutdown_from_events(
    events: &[DiagnosticsEvent],
    start_millis: u64,
    end_millis: u64,
) -> Option<aetower_model::RebootCauseSnapshot> {
    events
        .iter()
        .filter(|event| {
            event.timestamp_millis >= start_millis
                && event.timestamp_millis <= end_millis
                && matches!(
                    event.event_type.as_str(),
                    "system-previous-shutdown-cause" | "system-panic-marker"
                )
        })
        .max_by_key(|event| event.timestamp_millis)
        .map(|event| {
            let detail = event
                .fields
                .iter()
                .find(|field| field.key == "detail")
                .map(|field| field.value.clone())
                .unwrap_or_else(|| event.message.clone());
            let code = if event.event_type == "system-previous-shutdown-cause" {
                detail
                    .split(':')
                    .nth(1)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
            } else {
                Some("panic".to_owned())
            };
            aetower_model::RebootCauseSnapshot {
                source: "diagnostics".to_owned(),
                code,
                detail,
                observed_at_millis: Some(event.timestamp_millis),
            }
        })
}

fn snapshot_boot_key(host: &aetower_model::HostSnapshot) -> Option<String> {
    host.boot_session
        .as_ref()
        .and_then(|boot| boot.boot_id.clone())
        .or_else(|| {
            host.boot_session
                .as_ref()
                .and_then(|boot| boot.boot_time_millis)
                .map(|millis| millis.to_string())
        })
}

pub(crate) fn build_history_store_health(
    summary: HistorySummaryResponse,
    recent_history_events: Vec<DiagnosticsEvent>,
    data_quality: Option<HistoryDataQualityReport>,
) -> HistoryStoreHealth {
    let efficiency = build_history_efficiency_diagnostics(&summary, &recent_history_events);
    let severity = data_quality
        .as_ref()
        .map(|quality| {
            [
                history_store_severity(&summary),
                efficiency.severity,
                quality.severity,
            ]
            .into_iter()
            .max_by_key(|severity| severity.score())
            .unwrap_or(SeverityBand::Info)
        })
        .unwrap_or_else(|| history_store_severity(&summary).max(efficiency.severity));
    let mut thresholds = BTreeMap::new();
    thresholds.insert(
        "warning_store_bytes".to_owned(),
        json!(HISTORY_STORE_WARNING_BYTES),
    );
    thresholds.insert(
        "critical_store_bytes".to_owned(),
        json!(HISTORY_STORE_CRITICAL_BYTES),
    );
    thresholds.insert(
        "warning_wal_bytes".to_owned(),
        json!(HISTORY_WAL_WARNING_BYTES),
    );
    thresholds.insert(
        "critical_wal_bytes".to_owned(),
        json!(HISTORY_WAL_CRITICAL_BYTES),
    );
    thresholds.insert(
        "warning_quarantine_count".to_owned(),
        json!(HISTORY_QUARANTINE_WARNING),
    );
    thresholds.insert(
        "critical_quarantine_count".to_owned(),
        json!(HISTORY_QUARANTINE_CRITICAL),
    );
    let mut recommendations = Vec::new();
    if summary.store_bytes >= HISTORY_STORE_WARNING_BYTES {
        recommendations.push(
            "Trim persisted history more aggressively or shorten the default retention window."
                .to_owned(),
        );
    }
    if summary.wal_bytes >= HISTORY_WAL_WARNING_BYTES {
        recommendations.push(
            "Investigate write pressure and ensure checkpointing is keeping WAL growth under control."
                .to_owned(),
        );
    }
    if summary.quarantine_count >= HISTORY_QUARANTINE_WARNING {
        recommendations.push(
            "Inspect persisted history compatibility issues; quarantined rows should stay near zero."
                .to_owned(),
        );
    }
    if let Some(quality) = data_quality.as_ref()
        && quality.severity != SeverityBand::Info
    {
        recommendations.extend(quality.recommendations.clone());
    }
    recommendations.extend(efficiency.recommendations.clone());
    HistoryStoreHealth {
        severity,
        summary: format!(
            "{} DB, {} WAL, {} persisted snapshots, {} quarantined rows.",
            format_bytes(summary.store_bytes),
            format_bytes(summary.wal_bytes),
            summary.snapshot_count,
            summary.quarantine_count
        ),
        range: summary,
        efficiency,
        thresholds,
        data_quality,
        recent_history_events,
        recommendations,
    }
}

pub(crate) fn build_history_efficiency_diagnostics(
    summary: &HistorySummaryResponse,
    recent_history_events: &[DiagnosticsEvent],
) -> HistoryEfficiencyDiagnostics {
    let window_millis = match (summary.oldest_millis, summary.newest_millis) {
        (Some(oldest), Some(newest)) if newest > oldest => newest - oldest,
        _ => 0,
    };
    let window_minutes = (window_millis as f64 / 60_000.0).max(0.0);
    let write_rate_per_minute = if window_minutes > 0.0 {
        summary.range_count as f64 / window_minutes
    } else {
        0.0
    };
    let average_snapshot_bytes = if summary.snapshot_count > 0 {
        summary.store_bytes / summary.snapshot_count.max(1)
    } else {
        0
    };
    let estimated_bytes_per_minute = write_rate_per_minute * average_snapshot_bytes as f64;
    let store_pressure_percent = pressure_percent(summary.store_bytes, HISTORY_STORE_WARNING_BYTES);
    let wal_pressure_percent = pressure_percent(summary.wal_bytes, HISTORY_WAL_WARNING_BYTES);
    let snapshot_count_pressure_percent =
        pressure_percent(summary.snapshot_count, HISTORY_SNAPSHOT_WARNING_COUNT);
    let wal_age_millis = summary
        .wal_modified_millis
        .and_then(|modified| current_unix_millis().map(|now| now.saturating_sub(modified)));
    let checkpoint_status = latest_history_checkpoint_status(recent_history_events);
    let estimated_minutes_to_store_warning = estimated_minutes_to_limit(
        summary.store_bytes,
        HISTORY_STORE_WARNING_BYTES,
        estimated_bytes_per_minute,
    );
    let estimated_minutes_to_wal_warning = estimated_minutes_to_limit(
        summary.wal_bytes,
        HISTORY_WAL_WARNING_BYTES,
        estimated_bytes_per_minute,
    );

    let mut recommendations = Vec::new();
    if summary.pending_writes > 0 {
        recommendations.push(format!(
            "{} history write(s) are currently queued; watch for backlog before raising retention.",
            summary.pending_writes
        ));
    }
    if store_pressure_percent >= 80.0 {
        recommendations.push(
            "History DB is approaching its soft byte budget; shorten retention or increase checkpoint/trim cadence before it reaches the warning threshold."
                .to_owned(),
        );
    }
    if wal_pressure_percent >= 80.0 {
        recommendations.push(
            "History WAL is approaching its checkpoint budget; confirm maintenance is checkpointing under sustained write pressure."
                .to_owned(),
        );
    }
    if snapshot_count_pressure_percent >= 80.0 {
        recommendations.push(
            "Persisted snapshot count is approaching its soft cap; reduce write cadence or trim older samples."
                .to_owned(),
        );
    }
    if checkpoint_status.contains("No checkpoint") && summary.wal_bytes > 0 {
        recommendations.push(
            "No recent checkpoint event was found in diagnostics; keep persistence diagnostics enabled to prove WAL control."
                .to_owned(),
        );
    }

    let severity = if store_pressure_percent >= 100.0
        || wal_pressure_percent >= 100.0
        || summary.snapshot_count >= HISTORY_SNAPSHOT_CRITICAL_COUNT
    {
        SeverityBand::Critical
    } else if store_pressure_percent >= 80.0
        || wal_pressure_percent >= 80.0
        || snapshot_count_pressure_percent >= 80.0
        || summary.pending_writes > 0
    {
        SeverityBand::Warning
    } else {
        SeverityBand::Info
    };

    HistoryEfficiencyDiagnostics {
        severity,
        summary: format!(
            "{:.1} persisted sample(s)/min, average snapshot {}, DB pressure {:.0}%, WAL pressure {:.0}%.",
            write_rate_per_minute,
            format_bytes(average_snapshot_bytes),
            store_pressure_percent,
            wal_pressure_percent
        ),
        window_millis,
        write_rate_per_minute,
        estimated_bytes_per_minute,
        average_snapshot_bytes,
        store_pressure_percent,
        wal_pressure_percent,
        snapshot_count_pressure_percent,
        wal_age_millis,
        checkpoint_status,
        estimated_minutes_to_store_warning,
        estimated_minutes_to_wal_warning,
        recommendations,
    }
}

fn pressure_percent(current: u64, warning_limit: u64) -> f64 {
    if warning_limit == 0 {
        0.0
    } else {
        current as f64 / warning_limit as f64 * 100.0
    }
}

fn estimated_minutes_to_limit(current: u64, limit: u64, bytes_per_minute: f64) -> Option<f64> {
    if current >= limit {
        Some(0.0)
    } else if bytes_per_minute > 0.0 {
        Some((limit - current) as f64 / bytes_per_minute)
    } else {
        None
    }
}

fn latest_history_checkpoint_status(events: &[DiagnosticsEvent]) -> String {
    let checkpoint_event = events.iter().find(|event| {
        matches!(
            event.event_type.as_str(),
            "history-retention-maintained" | "history-maintained"
        )
    });
    let Some(event) = checkpoint_event else {
        return "No checkpoint event found in the selected diagnostics window.".to_owned();
    };
    let checkpointed = diagnostics_field_value(event, "checkpointed")
        .map(|value| value.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let wal_after = diagnostics_field_value(event, "wal_bytes_after")
        .and_then(|value| value.parse::<u64>().ok());
    if checkpointed {
        format!(
            "Checkpointed at {}; WAL after maintenance {}.",
            event.timestamp_millis,
            wal_after
                .map(format_bytes)
                .unwrap_or_else(|| "unknown".to_owned())
        )
    } else {
        format!(
            "Maintenance ran at {} but did not report a checkpoint.",
            event.timestamp_millis
        )
    }
}

fn diagnostics_field_value<'a>(event: &'a DiagnosticsEvent, key: &str) -> Option<&'a str> {
    event
        .fields
        .iter()
        .find(|field| field.key == key)
        .map(|field| field.value.as_str())
}
