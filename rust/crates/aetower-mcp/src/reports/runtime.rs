//! Runtime/self-observability report builders for the MCP server.

use aetower_diagnostics::DiagnosticsEvent;
use aetower_model::{RuntimeLagMetrics, SystemSnapshot};

use crate::*;

pub(crate) fn build_self_runtime_watch_report(
    data_source: &dyn AetowerMcpDataSource,
    duration_seconds: u64,
    interval_millis: u64,
    include_memory_breakdown: bool,
    top_regions: usize,
) -> Result<SelfRuntimeWatchReport, String> {
    let started_at_millis = current_unix_millis().unwrap_or_default();
    let started = Instant::now();
    let duration = Duration::from_secs(duration_seconds);
    let interval = Duration::from_millis(interval_millis);
    let mut samples = Vec::new();

    loop {
        let runtime = data_source.latest_runtime_lag_metrics()?;
        samples.push(self_runtime_watch_sample(&runtime));
        if started.elapsed() >= duration {
            break;
        }
        let remaining = duration.saturating_sub(started.elapsed());
        thread::sleep(interval.min(remaining));
    }

    let ended_at_millis = current_unix_millis().unwrap_or(started_at_millis);
    let averages = self_runtime_watch_averages(&samples);
    let memory = build_self_runtime_memory_attribution(
        data_source,
        &samples,
        include_memory_breakdown,
        top_regions,
    );
    let summary = self_runtime_watch_summary(&samples, &averages, &memory);

    Ok(SelfRuntimeWatchReport {
        started_at_millis,
        ended_at_millis,
        duration_seconds,
        interval_millis,
        sample_count: samples.len(),
        summary,
        averages,
        memory,
        samples,
    })
}

pub(crate) fn self_runtime_watch_sample(runtime: &RuntimeLagMetrics) -> SelfRuntimeWatchSample {
    SelfRuntimeWatchSample {
        observed_at_millis: current_unix_millis()
            .unwrap_or(runtime.updated_at_millis)
            .max(runtime.updated_at_millis),
        engine_tick_millis: runtime.engine_tick_millis,
        target_tick_millis: runtime.target_tick_millis,
        collect_millis: runtime.collect_millis,
        history_millis: runtime.history_millis,
        persist_millis: runtime.persist_millis,
        bridge_fetch_millis: runtime.bridge_fetch_millis,
        ui_refresh_millis: runtime.ui_refresh_millis,
        snapshot_to_render_millis: runtime.snapshot_to_render_millis,
        render_commit_millis: runtime.render_commit_millis,
        self_cpu_percent: runtime.self_cpu_percent,
        self_memory_bytes: runtime.self_memory_bytes,
        self_memory_physical_footprint_bytes: runtime.self_memory_physical_footprint_bytes,
        self_wakeups_per_second: runtime.self_wakeups_per_second,
        mcp_requests_per_second: runtime.mcp_requests_per_second,
        mcp_active_client_count: runtime.mcp_active_client_count,
        mcp_helper_count: runtime.mcp_helper_count,
        stale_mcp_helper_count: runtime.stale_mcp_helper_count,
    }
}

pub(crate) fn self_runtime_watch_averages(
    samples: &[SelfRuntimeWatchSample],
) -> SelfRuntimeWatchAverages {
    let count = samples.len().max(1) as f32;
    SelfRuntimeWatchAverages {
        engine_tick_millis: samples
            .iter()
            .map(|sample| sample.engine_tick_millis)
            .sum::<f32>()
            / count,
        collect_millis: samples
            .iter()
            .map(|sample| sample.collect_millis)
            .sum::<f32>()
            / count,
        history_millis: samples
            .iter()
            .map(|sample| sample.history_millis)
            .sum::<f32>()
            / count,
        persist_millis: samples
            .iter()
            .map(|sample| sample.persist_millis)
            .sum::<f32>()
            / count,
        self_cpu_percent: samples
            .iter()
            .map(|sample| sample.self_cpu_percent)
            .sum::<f32>()
            / count,
        self_wakeups_per_second: samples
            .iter()
            .map(|sample| sample.self_wakeups_per_second)
            .sum::<f32>()
            / count,
        mcp_requests_per_second: samples
            .iter()
            .map(|sample| sample.mcp_requests_per_second)
            .sum::<f32>()
            / count,
    }
}

pub(crate) fn build_self_runtime_memory_attribution(
    data_source: &dyn AetowerMcpDataSource,
    samples: &[SelfRuntimeWatchSample],
    include_memory_breakdown: bool,
    top_regions: usize,
) -> SelfRuntimeMemoryAttribution {
    let current = samples.last();
    let peak_resident = samples
        .iter()
        .max_by_key(|sample| sample.self_memory_bytes)
        .map(|sample| sample.self_memory_bytes)
        .unwrap_or_default();
    let peak_footprint_sample = samples
        .iter()
        .max_by_key(|sample| sample.self_memory_physical_footprint_bytes);
    let peak_footprint = peak_footprint_sample
        .map(|sample| sample.self_memory_physical_footprint_bytes)
        .unwrap_or_default();
    let peak_at_millis = peak_footprint_sample.map(|sample| sample.observed_at_millis);
    let current_footprint = current
        .map(|sample| sample.self_memory_physical_footprint_bytes)
        .unwrap_or_default();
    let current_resident = current
        .map(|sample| sample.self_memory_bytes)
        .unwrap_or_default();
    let self_process_id = std::process::id();
    let mut self_entity_id = None;
    let mut self_display_name = None;
    let mut process_ids = vec![self_process_id];
    let mut regions = Vec::new();
    let mut caveats = Vec::new();

    if include_memory_breakdown {
        match data_source.latest_snapshot() {
            Ok(snapshot) => {
                if let Some(entity) = self_entity_in_snapshot(&snapshot, self_process_id) {
                    self_entity_id = Some(entity.entity_id.clone());
                    self_display_name = Some(entity.display_name.clone());
                    process_ids = entity_process_ids(entity);
                } else {
                    caveats.push(
                        "Current Aetower process was not attributed to an entity; vmmap used the app PID directly."
                            .to_owned(),
                    );
                }
            }
            Err(error) => caveats.push(format!(
                "Could not load the current snapshot for self entity attribution: {error}."
            )),
        }

        match vmmap_regions_for_processes(&process_ids, top_regions.max(1)) {
            Ok(value) => regions = value,
            Err(error) => caveats.push(format!("Could not collect self vmmap regions: {error}.")),
        }
    } else {
        caveats.push("VM region breakdown was skipped by request.".to_owned());
    }

    SelfRuntimeMemoryAttribution {
        current_resident_bytes: current_resident,
        current_physical_footprint_bytes: current_footprint,
        peak_resident_bytes: peak_resident,
        peak_physical_footprint_bytes: peak_footprint,
        peak_at_millis,
        peak_delta_from_current_bytes: signed_byte_delta(peak_footprint, current_footprint),
        self_process_id,
        self_entity_id,
        self_display_name,
        process_ids,
        regions,
        note: "Resident bytes come from Aetower self telemetry; physical footprint follows macOS task footprint when available. VM regions are a point-in-time vmmap attribution for the app process or attributed self entity.".to_owned(),
        caveats,
    }
}

pub(crate) fn self_runtime_watch_summary(
    samples: &[SelfRuntimeWatchSample],
    averages: &SelfRuntimeWatchAverages,
    memory: &SelfRuntimeMemoryAttribution,
) -> String {
    let peak_tick = samples
        .iter()
        .map(|sample| sample.engine_tick_millis)
        .fold(0.0, f32::max);
    let peak_cpu = samples
        .iter()
        .map(|sample| sample.self_cpu_percent)
        .fold(0.0, f32::max);
    let peak_wakeups = samples
        .iter()
        .map(|sample| sample.self_wakeups_per_second)
        .fold(0.0, f32::max);
    format!(
        "{} samples: avg tick {:.1} ms, peak tick {:.1} ms, avg CPU {:.1}%, peak CPU {:.1}%, peak wakeups {:.0}/s, peak footprint {}.",
        samples.len(),
        averages.engine_tick_millis,
        peak_tick,
        averages.self_cpu_percent,
        peak_cpu,
        peak_wakeups,
        format_bytes(memory.peak_physical_footprint_bytes)
    )
}

pub(crate) fn self_entity_in_snapshot(
    snapshot: &SystemSnapshot,
    self_process_id: u32,
) -> Option<&aetower_model::EntitySnapshot> {
    snapshot.entities.iter().find(|entity| {
        entity
            .components
            .iter()
            .any(|component| component.process_id == Some(self_process_id))
    })
}

pub(crate) fn signed_byte_delta(after: u64, before: u64) -> i64 {
    let delta = after as i128 - before as i128;
    delta.clamp(i64::MIN as i128, i64::MAX as i128) as i64
}

pub(crate) fn build_runtime_burst_explanation(
    captured_at_millis: u64,
    window_minutes: u64,
    runtime: &RuntimeLagMetrics,
    events: &[DiagnosticsEvent],
) -> RuntimeBurstExplanation {
    let mut drivers = Vec::new();
    let target_tick = runtime.target_tick_millis.max(1.0);
    if runtime.engine_tick_millis >= target_tick * 0.75 {
        drivers.push(runtime_burst_driver(
            "engine-tick",
            SeverityBand::Critical,
            "engine_tick_millis",
            format!("{:.1} ms", runtime.engine_tick_millis),
            format!(
                "Engine tick is using {:.0}% of the active target cadence.",
                runtime.engine_tick_millis / target_tick * 100.0
            ),
            "Inspect collector and UI/render drivers before lowering cadence further.",
        ));
    } else if runtime.engine_tick_millis >= target_tick * 0.35 {
        drivers.push(runtime_burst_driver(
            "engine-tick",
            SeverityBand::Warning,
            "engine_tick_millis",
            format!("{:.1} ms", runtime.engine_tick_millis),
            format!(
                "Engine tick is using {:.0}% of the active target cadence.",
                runtime.engine_tick_millis / target_tick * 100.0
            ),
            "Watch for repeated over-budget ticks and correlate with adapter or history work.",
        ));
    }

    push_threshold_driver(
        &mut drivers,
        runtime.collect_millis,
        RuntimeBurstThreshold {
            key: "collect",
            metric: "collect_millis",
            warning_threshold: 75.0,
            critical_threshold: 150.0,
            detail: "Collector work is a visible part of this runtime sample.",
            recommendation: "Reduce scan breadth or increase cadence when live investigation does not need full collection.",
        },
    );
    push_threshold_driver(
        &mut drivers,
        runtime.history_millis,
        RuntimeBurstThreshold {
            key: "history",
            metric: "history_millis",
            warning_threshold: 50.0,
            critical_threshold: 100.0,
            detail: "History/timeline update work is elevated.",
            recommendation: "Inspect history efficiency and retention diagnostics before increasing persisted write rate.",
        },
    );
    push_threshold_driver(
        &mut drivers,
        runtime.persist_millis,
        RuntimeBurstThreshold {
            key: "persist",
            metric: "persist_millis",
            warning_threshold: 20.0,
            critical_threshold: 50.0,
            detail: "SQLite persistence work is elevated.",
            recommendation: "Check pending writes, WAL pressure, and checkpoint status.",
        },
    );
    push_threshold_driver(
        &mut drivers,
        runtime.snapshot_to_render_millis,
        RuntimeBurstThreshold {
            key: "ui-render",
            metric: "snapshot_to_render_millis",
            warning_threshold: 60.0,
            critical_threshold: 120.0,
            detail: "Snapshot-to-render latency is elevated.",
            recommendation: "Inspect expensive SwiftUI sections and avoid refreshing hidden tabs.",
        },
    );
    push_threshold_driver(
        &mut drivers,
        runtime.self_cpu_percent,
        RuntimeBurstThreshold {
            key: "self-cpu",
            metric: "self_cpu_percent",
            warning_threshold: 10.0,
            critical_threshold: 25.0,
            detail: "Aetower self CPU is elevated.",
            recommendation: "Run aetower_watch_self and correlate peaks with collector, adapter, history, and UI timings.",
        },
    );
    push_threshold_driver(
        &mut drivers,
        runtime.self_wakeups_per_second,
        RuntimeBurstThreshold {
            key: "self-wakeups",
            metric: "self_wakeups_per_second",
            warning_threshold: 300.0,
            critical_threshold: 1_000.0,
            detail: "Aetower self wakeups are elevated.",
            recommendation: "Audit polling, UI refresh, and MCP request bursts before adding more timers.",
        },
    );

    let mcp_severity = mcp_request_pressure_severity(runtime);
    if mcp_severity != SeverityBand::Info {
        drivers.push(runtime_burst_driver(
            "mcp-requests",
            mcp_severity,
            "mcp_requests_per_second",
            format!("{:.1}/s", runtime.mcp_requests_per_second),
            format!(
                "{} active client(s), {} helper(s), oldest helper {}.",
                runtime.mcp_active_client_count,
                runtime.mcp_helper_count,
                format_duration_millis(runtime.oldest_mcp_helper_age_millis)
            ),
            "Prefer persistent client sessions and avoid repeated short-lived MCP polling.",
        ));
    }
    if runtime.history_queue_depth > 0 || runtime.diagnostics_queue_depth > 0 {
        let queue_depth = runtime
            .history_queue_depth
            .saturating_add(runtime.diagnostics_queue_depth);
        drivers.push(runtime_burst_driver(
            "queues",
            if queue_depth >= 10 {
                SeverityBand::Critical
            } else {
                SeverityBand::Warning
            },
            "queue_depth",
            queue_depth.to_string(),
            format!(
                "History queue {}, diagnostics queue {}.",
                runtime.history_queue_depth, runtime.diagnostics_queue_depth
            ),
            "Keep write queues near zero; sustained backlog means observer work is falling behind.",
        ));
    }

    let adapter_refresh_count = events
        .iter()
        .filter(|event| event.event_type.starts_with("adapter-refresh"))
        .count();
    let adapter_failed_count = events
        .iter()
        .filter(|event| event.event_type == "adapter-refresh-failed")
        .count();
    if adapter_failed_count > 0 || adapter_refresh_count >= 20 {
        drivers.push(runtime_burst_driver(
            "adapter-refresh",
            if adapter_failed_count > 0 {
                SeverityBand::Warning
            } else {
                SeverityBand::Info
            },
            "adapter_refresh_events",
            adapter_refresh_count.to_string(),
            format!(
                "{} adapter refresh event(s), {} failure(s), in the selected window.",
                adapter_refresh_count, adapter_failed_count
            ),
            "If collection time spikes align with adapter refreshes, lengthen adapter cadence or cache expensive adapter calls.",
        ));
    }

    let mut correlated_events = events
        .iter()
        .filter(|event| runtime_event_is_correlated(event))
        .take(20)
        .map(runtime_correlated_event)
        .collect::<Vec<_>>();
    correlated_events.sort_by(|left, right| right.timestamp_millis.cmp(&left.timestamp_millis));
    drivers.sort_by(|left, right| {
        right
            .severity
            .score()
            .cmp(&left.severity.score())
            .then_with(|| left.key.cmp(&right.key))
    });
    let severity = drivers
        .iter()
        .map(|driver| driver.severity)
        .max_by_key(|severity| severity.score())
        .unwrap_or(SeverityBand::Info);
    let mut recommendations = Vec::new();
    for driver in &drivers {
        if !recommendations.contains(&driver.recommendation) {
            recommendations.push(driver.recommendation.clone());
        }
    }
    RuntimeBurstExplanation {
        captured_at_millis,
        window_minutes,
        severity,
        summary: if let Some(driver) = drivers.first() {
            format!(
                "{} runtime burst driver(s); strongest signal is {} at {}.",
                drivers.len(),
                driver.metric,
                driver.value
            )
        } else {
            "No dominant runtime burst driver is visible in the current sample.".to_owned()
        },
        drivers,
        correlated_events,
        recommendations,
    }
}

pub(crate) fn push_threshold_driver(
    drivers: &mut Vec<RuntimeBurstDriver>,
    value: f32,
    threshold: RuntimeBurstThreshold<'_>,
) {
    if value >= threshold.critical_threshold {
        drivers.push(runtime_burst_driver(
            threshold.key,
            SeverityBand::Critical,
            threshold.metric,
            format!("{value:.1}"),
            threshold.detail,
            threshold.recommendation,
        ));
    } else if value >= threshold.warning_threshold {
        drivers.push(runtime_burst_driver(
            threshold.key,
            SeverityBand::Warning,
            threshold.metric,
            format!("{value:.1}"),
            threshold.detail,
            threshold.recommendation,
        ));
    }
}

pub(crate) fn runtime_burst_driver(
    key: &str,
    severity: SeverityBand,
    metric: &str,
    value: String,
    detail: impl Into<String>,
    recommendation: &str,
) -> RuntimeBurstDriver {
    RuntimeBurstDriver {
        key: key.to_owned(),
        severity,
        metric: metric.to_owned(),
        value,
        detail: detail.into(),
        recommendation: recommendation.to_owned(),
    }
}

pub(crate) fn runtime_event_is_correlated(event: &DiagnosticsEvent) -> bool {
    matches!(
        event.subsystem,
        DiagnosticsSubsystem::Engine
            | DiagnosticsSubsystem::History
            | DiagnosticsSubsystem::Persistence
            | DiagnosticsSubsystem::Ui
            | DiagnosticsSubsystem::AdapterChromium
            | DiagnosticsSubsystem::AdapterDocker
            | DiagnosticsSubsystem::AdapterHelper
            | DiagnosticsSubsystem::AdapterChau7
            | DiagnosticsSubsystem::AdapterVsCode
    ) || event.event_type.contains("mcp")
        || event.event_type.contains("history")
        || event.event_type.contains("adapter-refresh")
}

pub(crate) fn runtime_correlated_event(event: &DiagnosticsEvent) -> RuntimeBurstCorrelatedEvent {
    RuntimeBurstCorrelatedEvent {
        timestamp_millis: event.timestamp_millis,
        level: format!("{:?}", event.level),
        subsystem: format!("{:?}", event.subsystem),
        event_type: event.event_type.clone(),
        message: event.message.clone(),
        detail: event
            .fields
            .iter()
            .find(|field| field.key == "error" || field.key == "aggressive_reason")
            .map(|field| field.value.clone()),
    }
}

pub(crate) fn build_session_health_checks(
    snapshot: &SystemSnapshot,
    diagnostics: &DiagnosticsOverview,
    runtime: &RuntimeLagMetrics,
    history: &HistorySummaryResponse,
    history_events: &[DiagnosticsEvent],
    runtime_events: &[DiagnosticsEvent],
) -> Vec<SessionHealthCheck> {
    let history_efficiency = build_history_efficiency_diagnostics(history, history_events);
    let runtime_burst = build_runtime_burst_explanation(
        snapshot.captured_at_millis,
        DEFAULT_RUNTIME_BURST_WINDOW_MINUTES,
        runtime,
        runtime_events,
    );
    let mut checks = vec![
        SessionHealthCheck {
            key: "runtime".to_owned(),
            severity: runtime_severity(runtime),
            summary: format!(
                "Engine tick {:.1} ms against a {:.0} ms target.",
                runtime.engine_tick_millis, runtime.target_tick_millis
            ),
            detail: format!(
                "Runtime sample {}, collect {:.1} ms, history {:.1} ms, persist {:.1} ms, history queue {}, diagnostics queue {}, MCP helpers {} ({} stale). Aetower self: CPU {:.1}%, memory {}, wakeups {:.0}/s.",
                runtime.updated_at_millis,
                runtime.collect_millis,
                runtime.history_millis,
                runtime.persist_millis,
                runtime.history_queue_depth,
                runtime.diagnostics_queue_depth,
                runtime.mcp_helper_count,
                runtime.stale_mcp_helper_count,
                runtime.self_cpu_percent,
                format_bytes(runtime.self_memory_bytes),
                runtime.self_wakeups_per_second
            ),
        },
        SessionHealthCheck {
            key: "diagnostics".to_owned(),
            severity: diagnostics_severity(diagnostics),
            summary: format!(
                "{} warnings, {} errors, {} persisted diagnostics events.",
                diagnostics.warn_count, diagnostics.error_count, diagnostics.persisted_events
            ),
            detail: diagnostics_active_error_message(diagnostics).unwrap_or_else(|| {
                if diagnostics.error_count > 0 {
                    "Retained diagnostics errors are stale or already recovered.".to_owned()
                } else {
                    "No current diagnostics error is attached.".to_owned()
                }
            }),
        },
        SessionHealthCheck {
            key: "history-store".to_owned(),
            severity: history_store_severity(history).max(history_efficiency.severity),
            summary: format!(
                "{} DB, {} WAL, {} persisted snapshots, {:.1} samples/min.",
                format_bytes(history.store_bytes),
                format_bytes(history.wal_bytes),
                history.snapshot_count,
                history_efficiency.write_rate_per_minute
            ),
            detail: format!(
                "{} quarantined rows, average snapshot {}, DB pressure {:.0}%, WAL pressure {:.0}%, checkpoint: {}",
                history.quarantine_count,
                format_bytes(history_efficiency.average_snapshot_bytes),
                history_efficiency.store_pressure_percent,
                history_efficiency.wal_pressure_percent,
                history_efficiency.checkpoint_status
            ),
        },
        SessionHealthCheck {
            key: "capabilities".to_owned(),
            severity: snapshot
                .capabilities
                .iter()
                .map(capability_severity)
                .max_by_key(|severity| severity.score())
                .unwrap_or(SeverityBand::Info),
            summary: format!(
                "{} capability checks are currently tracked.",
                snapshot.capabilities.len()
            ),
            detail: snapshot
                .capabilities
                .iter()
                .map(|capability| format!("{:?}: {:?}", capability.kind, capability.state))
                .collect::<Vec<_>>()
                .join(", "),
        },
        SessionHealthCheck {
            key: "host-load".to_owned(),
            severity: host_load_severity(snapshot),
            summary: format!(
                "{} used / {}, {} compressed, {} swap, {:.0} wakeups/s.",
                format_bytes(snapshot.host.memory_used_bytes),
                format_bytes(snapshot.host.memory_total_bytes),
                format_bytes(snapshot.host.compressed_memory_bytes),
                format_bytes(snapshot.host.swap_used_bytes),
                snapshot.host.wakeups_per_second
            ),
            detail: host_load_detail(snapshot),
        },
        SessionHealthCheck {
            key: "mcp".to_owned(),
            severity: mcp_helper_severity(runtime),
            summary: if runtime.mcp_helper_count == 0 {
                format!(
                    "The local in-app MCP server is serving the current app-owned engine state with {} active client(s).",
                    runtime.mcp_active_client_count
                )
            } else {
                format!(
                    "The local in-app MCP server is serving current engine state with {} helper processes ({} stale) and {} active client(s).",
                    runtime.mcp_helper_count,
                    runtime.stale_mcp_helper_count,
                    runtime.mcp_active_client_count
                )
            },
            detail: if runtime.stale_mcp_helper_count > 0 {
                format!(
                    "{} helper processes have been alive for at least {} minutes; oldest helper age is {}. Helpers should exit when clients disconnect.",
                    runtime.stale_mcp_helper_count,
                    MCP_HELPER_STALE_MILLIS / 60_000,
                    format_duration_millis(runtime.oldest_mcp_helper_age_millis)
                )
            } else if runtime.mcp_helper_count > 0 {
                format!(
                    "Oldest helper age is {}; helpers older than {} minutes become lifecycle signals. Total MCP requests: {}, current rate: {:.1}/s.",
                    format_duration_millis(runtime.oldest_mcp_helper_age_millis),
                    MCP_HELPER_STALE_MILLIS / 60_000,
                    runtime.mcp_total_requests,
                    runtime.mcp_requests_per_second
                )
            } else {
                format!(
                    "Agents should consume these tools instead of starting a second collector process. Total MCP requests: {}, current rate: {:.1}/s.",
                    runtime.mcp_total_requests, runtime.mcp_requests_per_second
                )
            },
        },
    ];
    checks.push(SessionHealthCheck {
        key: "runtime-bursts".to_owned(),
        severity: runtime_burst.severity,
        summary: runtime_burst.summary,
        detail: if runtime_burst.drivers.is_empty() {
            "No dominant Aetower runtime burst driver is visible in the current sample.".to_owned()
        } else {
            runtime_burst
                .drivers
                .iter()
                .map(|driver| format!("{}: {}", driver.metric, driver.detail))
                .collect::<Vec<_>>()
                .join(" ")
        },
    });
    checks
}

pub(crate) struct AiRuntimeReportData {
    pub(crate) summary: AiRuntimeSummary,
    pub(crate) burden_leaders: Vec<AiBurdenLeaderReport>,
    pub(crate) runtime_groups: Vec<AiRuntimeGroupReport>,
    pub(crate) approvals: Vec<AiApprovalReport>,
    pub(crate) delegations: Vec<AiDelegationReport>,
    pub(crate) recent_changes: Vec<RecentChangeItem>,
    pub(crate) historical_groups: Vec<AiHistoricalTrendReport>,
    pub(crate) recommendations: Vec<RecommendationItem>,
}

pub(crate) fn build_ai_runtime_report(
    snapshot: &SystemSnapshot,
    history: &[SystemSnapshot],
) -> AiRuntimeReportData {
    let ai_entities = snapshot
        .entities
        .iter()
        .filter(|entity| matches!(entity.entity_kind, aetower_model::EntityKind::AiAgent))
        .collect::<Vec<_>>();

    let runtime_groups = ai_runtime_groups(&ai_entities);
    let group_keys = runtime_groups
        .iter()
        .map(|group| (group.provider.clone(), group.workspace.clone()))
        .collect::<Vec<_>>();

    AiRuntimeReportData {
        summary: AiRuntimeSummary {
            agent_count: ai_entities.len(),
            total_energy_nj_per_s: ai_entities
                .iter()
                .map(|entity| entity.metrics.energy_nj_per_s)
                .sum(),
            total_cost_usd: ai_entities
                .iter()
                .filter_map(|entity| entity.agent_cost.as_ref().map(|cost| cost.cost_usd))
                .sum(),
            total_session_energy_nj: ai_entities
                .iter()
                .filter_map(|entity| {
                    entity
                        .agent_cost
                        .as_ref()
                        .map(|cost| cost.session_energy_nj)
                })
                .sum(),
            host_gpu_percent: snapshot.host.gpu_percent,
            host_gpu_memory_unified_percent: if snapshot.host.memory_total_bytes == 0 {
                0.0
            } else {
                snapshot.host.gpu_memory_bytes as f64 / snapshot.host.memory_total_bytes as f64
                    * 100.0
            },
        },
        burden_leaders: ai_burden_leaders(&ai_entities),
        approvals: ai_approval_queue(&ai_entities),
        delegations: ai_delegations(&ai_entities),
        recent_changes: ai_recent_changes(snapshot, &ai_entities),
        historical_groups: ai_historical_groups(history, &group_keys),
        recommendations: ai_recommendations(&ai_entities),
        runtime_groups,
    }
}

pub(crate) fn load_ai_runtime_history(
    data_source: &dyn AetowerMcpDataSource,
    start_millis: u64,
    end_millis: u64,
    limit: u32,
) -> (Vec<SystemSnapshot>, Option<String>) {
    match data_source.load_history_page(start_millis, end_millis, None, limit) {
        Ok(history) => (history, None),
        Err(error) => (
            Vec::new(),
            Some(format!(
                "Historical AI runtime trends are temporarily unavailable: {error}"
            )),
        ),
    }
}
