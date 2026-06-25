use std::{
    collections::BTreeMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use super::*;

const CHAU7_LOG_TAIL_BYTES: u64 = 2 * 1024 * 1024;
const MAX_RENDER_PIPELINE_SAMPLES: usize = 8;
const MAX_RENDER_VIEW_SAMPLES: usize = 16;

pub(crate) fn build_entity_profile(
    data_source: &dyn AetowerMcpDataSource,
    entity_id: &str,
    duration_seconds: u64,
    top_stacks: usize,
) -> Result<EntityProfileReport, String> {
    let snapshot = data_source.latest_snapshot()?;
    let entity = snapshot
        .entities
        .iter()
        .find(|entity| entity.entity_id == entity_id)
        .ok_or_else(|| format!("Unknown entity_id: {entity_id}"))?;
    let process_ids = entity_process_ids(entity);
    if process_ids.is_empty() {
        return Err(format!(
            "Entity {} has no attributed process IDs to profile.",
            entity.display_name
        ));
    }
    let mut stack_reports = Vec::new();
    for pid in &process_ids {
        let output = run_os_command(
            "/usr/bin/sample",
            &[
                pid.to_string(),
                duration_seconds.to_string(),
                "1".to_owned(),
            ],
        )?;
        stack_reports.extend(parse_sample_threads(&output));
    }
    stack_reports.sort_by(|left, right| right.sample_count.cmp(&left.sample_count));
    stack_reports.truncate(top_stacks.max(1));
    let summary = if let Some(first) = stack_reports.first() {
        format!(
            "Top sampled thread {} accounted for {} samples and is classified as {}.",
            first.thread_label, first.sample_count, first.classification
        )
    } else {
        "No non-empty sampled stacks were captured.".to_owned()
    };
    Ok(EntityProfileReport {
        captured_at_millis: snapshot.captured_at_millis,
        entity_id: entity.entity_id.clone(),
        display_name: entity.display_name.clone(),
        duration_seconds,
        sampled_process_ids: process_ids,
        thread_count: stack_reports.len(),
        top_stacks: stack_reports,
        summary,
    })
}

pub fn profile_entity_json(
    data_source: &dyn AetowerMcpDataSource,
    entity_id: &str,
    duration_seconds: u64,
    top_stacks: usize,
) -> Result<String, String> {
    let report = build_entity_profile(
        data_source,
        entity_id,
        duration_seconds.max(1),
        top_stacks.max(1),
    )?;
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

pub(crate) fn build_wakeup_attribution(
    data_source: &dyn AetowerMcpDataSource,
    entity_id: &str,
    duration_seconds: u64,
    top_stacks: usize,
) -> Result<WakeupAttributionReport, String> {
    let snapshot = data_source.latest_snapshot()?;
    let entity = snapshot
        .entities
        .iter()
        .find(|entity| entity.entity_id == entity_id)
        .ok_or_else(|| format!("Unknown entity_id: {entity_id}"))?;
    let process_wakeups_per_second = entity.metrics.wakeups_per_second;
    let display_link_state = build_display_link_state_report(entity);

    let profile =
        build_entity_profile(data_source, entity_id, duration_seconds, top_stacks.max(3))?;
    let mut grouped = BTreeMap::<(String, String), SampledStackReport>::new();
    for stack in &profile.top_stacks {
        let key = (
            stack
                .queue_label
                .clone()
                .unwrap_or_else(|| stack.thread_label.clone()),
            stack.classification.clone(),
        );
        let entry = grouped.entry(key).or_insert(SampledStackReport {
            thread_label: stack.thread_label.clone(),
            queue_label: stack.queue_label.clone(),
            sample_count: 0,
            top_frames: stack.top_frames.clone(),
            classification: stack.classification.clone(),
        });
        entry.sample_count += stack.sample_count;
        if entry.top_frames.is_empty() {
            entry.top_frames = stack.top_frames.clone();
        }
    }
    let mut queue_breakdown = grouped.into_values().collect::<Vec<_>>();
    queue_breakdown.sort_by(|left, right| right.sample_count.cmp(&left.sample_count));
    queue_breakdown.truncate(top_stacks.max(1));
    let timer_inventory = build_timer_inventory_report(&profile.top_stacks);
    let exact_thread_wakeup_counters = WakeupDataSourceStatus {
        key: "per-thread-wakeup-counters".to_owned(),
        title: "Per-thread wakeup counters".to_owned(),
        status: "unavailable-with-public-proc-api".to_owned(),
        detail: "macOS proc_pid_rusage exposes process-level wakeup counts, but not wakeup deltas per thread. Aetower therefore reports exact process wakeups plus sampled hot threads unless a privileged trace sampler or app-side instrumentation is available.".to_owned(),
        next_action: Some("Add a privileged, opt-in sampler or instrument the target app to publish per-thread/timer counters.".to_owned()),
    };
    let call_stack_sampling = WakeupDataSourceStatus {
        key: "call-stack-sampling".to_owned(),
        title: "Wakeup call-stack sampling".to_owned(),
        status: "available-heuristic".to_owned(),
        detail: format!(
            "Collected a bounded {}s sample and summarized the hottest threads/queues. Sample counts are time-on-stack, not exact wakeup events.",
            profile.duration_seconds
        ),
        next_action: None,
    };
    let process_wakeup_source = WakeupDataSourceStatus {
        key: "process-wakeups".to_owned(),
        title: "Process wakeups".to_owned(),
        status: "available-exact-process-delta".to_owned(),
        detail: format!(
            "Current entity wakeup rate from proc_pid_rusage/process aggregation is {:.0}/s.",
            process_wakeups_per_second
        ),
        next_action: None,
    };
    let timer_status = WakeupDataSourceStatus {
        key: "timer-inventory".to_owned(),
        title: "Timer inventory".to_owned(),
        status: timer_inventory.status.clone(),
        detail: timer_inventory.detail.clone(),
        next_action: Some("Expose active timers from the app runtime with timer id, owner, interval, tolerance, queue, and last-fire timestamp.".to_owned()),
    };
    let display_link_status = WakeupDataSourceStatus {
        key: "display-link-state".to_owned(),
        title: "Display-link/render state".to_owned(),
        status: display_link_state.status.clone(),
        detail: display_link_state.detail.clone(),
        next_action: Some("For exact state, have the app adapter publish per-view display-link isPaused/isRunning, FPS target, last-draw, and invalidation reason.".to_owned()),
    };
    let dominant_cause = queue_breakdown
        .iter()
        .find(|entry| entry.classification != "idle")
        .map(|entry| {
            format!(
                "{} dominates sampled wakeups with {} samples on {}.",
                entry.classification,
                entry.sample_count,
                entry
                    .queue_label
                    .clone()
                    .unwrap_or_else(|| entry.thread_label.clone())
            )
        });
    Ok(WakeupAttributionReport {
        captured_at_millis: profile.captured_at_millis,
        entity_id: profile.entity_id,
        display_name: profile.display_name,
        duration_seconds: profile.duration_seconds,
        sampled_process_ids: profile.sampled_process_ids,
        queue_breakdown: queue_breakdown.clone(),
        dominant_cause,
        attribution_mode: "sampled-call-stack-heuristic".to_owned(),
        process_wakeups_per_second,
        sampled_thread_breakdown: queue_breakdown,
        exact_thread_wakeup_counters: exact_thread_wakeup_counters.clone(),
        timer_inventory,
        display_link_state,
        data_sources: vec![
            process_wakeup_source,
            exact_thread_wakeup_counters,
            call_stack_sampling,
            timer_status,
            display_link_status,
        ],
        caveats: vec![
            "Sampled thread percentages are not exact wakeup counters; they identify where CPU/render time was spent during the capture.".to_owned(),
            "Queue labels are only present when the sampled stack exposed one.".to_owned(),
            "Exact timer inventory and per-view display-link state require target-app instrumentation or an explicitly approved privileged sampler.".to_owned(),
        ],
    })
}

pub fn wakeup_attribution_json(
    data_source: &dyn AetowerMcpDataSource,
    entity_id: &str,
    duration_seconds: u64,
    top_stacks: usize,
) -> Result<String, String> {
    let report = build_wakeup_attribution(
        data_source,
        entity_id,
        duration_seconds.max(1),
        top_stacks.max(1),
    )?;
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

pub(crate) fn build_process_sample(
    pid: u32,
    duration_seconds: u64,
    top_stacks: usize,
) -> Result<ProcessSampleReport, String> {
    validate_pid(pid)?;
    if !process_exists(pid) {
        return Err(format!("Process {pid} is not visible to ps right now."));
    }
    let output = run_os_command(
        "/usr/bin/sample",
        &[
            pid.to_string(),
            duration_seconds.to_string(),
            "1".to_owned(),
        ],
    )?;
    let mut stack_reports = parse_sample_threads(&output);
    stack_reports.sort_by(|left, right| right.sample_count.cmp(&left.sample_count));
    stack_reports.truncate(top_stacks.max(1));
    let summary = if let Some(first) = stack_reports.first() {
        format!(
            "Top sampled thread {} accounted for {} samples and is classified as {}.",
            first.thread_label, first.sample_count, first.classification
        )
    } else {
        "No non-empty sampled stacks were captured.".to_owned()
    };
    Ok(ProcessSampleReport {
        captured_at_millis: current_unix_millis().unwrap_or_default(),
        pid,
        duration_seconds,
        thread_count: stack_reports.len(),
        top_stacks: stack_reports,
        summary,
    })
}

pub fn process_sample_json(
    pid: u32,
    duration_seconds: u64,
    top_stacks: usize,
) -> Result<String, String> {
    let report = build_process_sample(
        pid,
        duration_seconds.clamp(1, MAX_PROFILE_DURATION_SECONDS),
        top_stacks.max(1),
    )?;
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

pub(crate) fn parse_sample_threads(output: &str) -> Vec<SampledStackReport> {
    let mut threads = Vec::new();
    let mut in_call_graph = false;
    let mut current: Option<(u32, String, Option<String>, Vec<String>)> = None;
    for line in output.lines() {
        if line.trim() == "Call graph:" {
            in_call_graph = true;
            continue;
        }
        if !in_call_graph {
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        if !line.starts_with(' ') && !line.starts_with('\t') {
            break;
        }
        if let Some((sample_count, thread_label, queue_label)) = parse_sample_thread_header(line) {
            if let Some((sample_count, thread_label, queue_label, frames)) = current.take() {
                threads.push(sampled_stack_report(
                    sample_count,
                    thread_label,
                    queue_label,
                    frames,
                ));
            }
            current = Some((sample_count, thread_label, queue_label, Vec::new()));
            continue;
        }
        if let Some((_, _, _, frames)) = current.as_mut()
            && let Some(frame) = parse_sample_frame(line)
        {
            frames.push(frame);
        }
    }
    if let Some((sample_count, thread_label, queue_label, frames)) = current.take() {
        threads.push(sampled_stack_report(
            sample_count,
            thread_label,
            queue_label,
            frames,
        ));
    }
    threads
}

pub(crate) fn parse_sample_thread_header(line: &str) -> Option<(u32, String, Option<String>)> {
    let trimmed = line.trim_start();
    let mut parts = trimmed.split_whitespace();
    let sample_count = parts.next()?.parse::<u32>().ok()?;
    let rest = trimmed[trimmed.find(' ')?..].trim_start();
    if !rest.starts_with("Thread_") {
        return None;
    }
    let thread_label = rest.to_owned();
    let queue_label = rest
        .split_once(": ")
        .map(|(_, queue)| queue.trim_end_matches("  (serial)").trim().to_owned());
    Some((sample_count, thread_label, queue_label))
}

pub(crate) fn parse_sample_frame(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let frame = trimmed
        .trim_start_matches('+')
        .trim_start_matches('!')
        .trim_start();
    let mut parts = frame.split("  (in ");
    let symbol = parts.next()?.trim();
    if symbol.is_empty() {
        return None;
    }
    let symbol = symbol
        .split_once(' ')
        .map(|(_, rest)| rest.trim())
        .unwrap_or(symbol);
    if symbol.is_empty() {
        None
    } else {
        Some(symbol.to_owned())
    }
}

pub(crate) fn sampled_stack_report(
    sample_count: u32,
    thread_label: String,
    queue_label: Option<String>,
    frames: Vec<String>,
) -> SampledStackReport {
    let top_frames = frames
        .into_iter()
        .filter(|frame| {
            !frame.contains("mach_msg")
                && !frame.contains("kevent")
                && !frame.contains("start")
                && !frame.contains("thread_start")
        })
        .take(6)
        .collect::<Vec<_>>();
    let classification = classify_sample_frames(&top_frames);
    SampledStackReport {
        thread_label,
        queue_label,
        sample_count,
        top_frames,
        classification,
    }
}

pub(crate) fn classify_sample_frames(frames: &[String]) -> String {
    let joined = frames.join(" ").to_ascii_lowercase();
    if joined.contains("mtkview draw")
        || joined.contains("rustmetaldisplaycoordinator.draw")
        || joined.contains("metalterminalrenderer.render")
    {
        "metal-render-loop".to_owned()
    } else if joined.contains("cvdisplaylink") || joined.contains("displaylink") {
        "display-link".to_owned()
    } else if joined.contains("dispatchsourcetimer")
        || joined.contains("dispatch_source")
        || joined.contains("timer")
    {
        "timer".to_owned()
    } else if joined.contains("nsrunloop") || joined.contains("cfrunlooptimer") {
        "runloop-timer".to_owned()
    } else if joined.contains("recv")
        || joined.contains("send")
        || joined.contains("socket")
        || joined.contains("poll")
    {
        "io".to_owned()
    } else if joined.is_empty() {
        "idle".to_owned()
    } else {
        "cpu-work".to_owned()
    }
}

fn build_timer_inventory_report(stacks: &[SampledStackReport]) -> TimerInventoryReport {
    let inferred_timer_threads = stacks
        .iter()
        .filter(|stack| matches!(stack.classification.as_str(), "timer" | "runloop-timer"))
        .cloned()
        .collect::<Vec<_>>();
    let status = if inferred_timer_threads.is_empty() {
        "requires-instrumentation".to_owned()
    } else {
        "sampled-inference".to_owned()
    };
    let detail = if inferred_timer_threads.is_empty() {
        "No timer stack dominated this bounded sample. Aetower cannot enumerate live DispatchSourceTimer/NSTimer instances from another process without app instrumentation or a privileged trace path.".to_owned()
    } else {
        format!(
            "Detected {} sampled timer-related thread(s). This is not a complete timer inventory.",
            inferred_timer_threads.len()
        )
    };
    TimerInventoryReport {
        status,
        detail,
        inferred_timer_threads,
        recommended_integration_fields: vec![
            "timer_id".to_owned(),
            "owner_component".to_owned(),
            "interval_millis".to_owned(),
            "tolerance_millis".to_owned(),
            "queue_label".to_owned(),
            "last_fire_millis".to_owned(),
            "next_fire_millis".to_owned(),
        ],
    }
}

fn build_display_link_state_report(
    entity: &aetower_model::EntitySnapshot,
) -> DisplayLinkStateReport {
    if !is_chau7_entity(entity) {
        return DisplayLinkStateReport {
            status: "not-applicable".to_owned(),
            detail: "No app-specific display-link adapter is available for this entity. Generic sampling can still classify display-link or Metal render stacks when they appear in the call sample.".to_owned(),
            source: None,
            latest_pipeline: None,
            recent_pipeline: Vec::new(),
            views: Vec::new(),
            recommended_integration_fields: display_link_recommended_fields(),
        };
    }

    let Some(log_path) = chau7_log_path() else {
        return DisplayLinkStateReport {
            status: "log-unavailable".to_owned(),
            detail: "Could not resolve ~/Library/Logs/Chau7.log. Exact display-link state still needs Chau7 adapter instrumentation.".to_owned(),
            source: None,
            latest_pipeline: None,
            recent_pipeline: Vec::new(),
            views: Vec::new(),
            recommended_integration_fields: display_link_recommended_fields(),
        };
    };

    match read_file_tail(&log_path, CHAU7_LOG_TAIL_BYTES) {
        Ok(tail) => display_link_report_from_chau7_log(&log_path, &tail),
        Err(error) => DisplayLinkStateReport {
            status: "log-unavailable".to_owned(),
            detail: format!(
                "Could not read {}: {error}. Exact display-link state still needs Chau7 adapter instrumentation.",
                log_path.display()
            ),
            source: Some(log_path.display().to_string()),
            latest_pipeline: None,
            recent_pipeline: Vec::new(),
            views: Vec::new(),
            recommended_integration_fields: display_link_recommended_fields(),
        },
    }
}

fn is_chau7_entity(entity: &aetower_model::EntitySnapshot) -> bool {
    entity.display_name.eq_ignore_ascii_case("chau7")
        || entity.entity_id.contains("/chau7.app")
        || entity
            .executable_path
            .as_deref()
            .is_some_and(|path| path.contains("/Chau7.app/"))
}

fn chau7_log_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join("Library/Logs/Chau7.log"))
}

fn read_file_tail(path: &Path, max_bytes: u64) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let len = file.metadata().map_err(|error| error.to_string())?.len();
    let offset = len.saturating_sub(max_bytes);
    file.seek(SeekFrom::Start(offset))
        .map_err(|error| error.to_string())?;
    let mut buffer = String::new();
    file.read_to_string(&mut buffer)
        .map_err(|error| error.to_string())?;
    if offset > 0
        && let Some(first_newline) = buffer.find('\n')
    {
        buffer.drain(..=first_newline);
    }
    Ok(buffer)
}

fn display_link_report_from_chau7_log(path: &Path, log_tail: &str) -> DisplayLinkStateReport {
    let mut recent_pipeline = log_tail
        .lines()
        .filter_map(parse_chau7_render_pipeline_line)
        .collect::<Vec<_>>();
    let mut views = log_tail
        .lines()
        .flat_map(parse_chau7_render_live_views_line)
        .collect::<Vec<_>>();

    if recent_pipeline.len() > MAX_RENDER_PIPELINE_SAMPLES {
        recent_pipeline =
            recent_pipeline.split_off(recent_pipeline.len() - MAX_RENDER_PIPELINE_SAMPLES);
    }
    if views.len() > MAX_RENDER_VIEW_SAMPLES {
        views = views.split_off(views.len() - MAX_RENDER_VIEW_SAMPLES);
    }

    let latest_pipeline = recent_pipeline.last().cloned();
    let status = if latest_pipeline.is_some() {
        "available-log-derived".to_owned()
    } else {
        "requires-integration".to_owned()
    };
    let detail = if let Some(latest) = latest_pipeline.as_ref() {
        format!(
            "Latest Chau7 render window reported {} live view(s), {} polls, {} draws, and {:.1} MiB sync. This is render-loop state from Chau7 logs, not raw CVDisplayLink isRunning state.",
            latest.live_views, latest.polls, latest.draws, latest.sync_mib
        )
    } else {
        "No recent Chau7 render telemetry was found in the log tail. Ask Chau7 to expose per-view display-link state through its MCP adapter.".to_owned()
    };

    DisplayLinkStateReport {
        status,
        detail,
        source: Some(path.display().to_string()),
        latest_pipeline,
        recent_pipeline,
        views,
        recommended_integration_fields: display_link_recommended_fields(),
    }
}

fn display_link_recommended_fields() -> Vec<String> {
    vec![
        "view_id".to_owned(),
        "tab_id".to_owned(),
        "session_id".to_owned(),
        "is_visible".to_owned(),
        "is_interactive".to_owned(),
        "display_link_running".to_owned(),
        "target_fps".to_owned(),
        "last_draw_millis".to_owned(),
        "last_invalidation_reason".to_owned(),
        "polls_per_second".to_owned(),
        "draws_per_second".to_owned(),
    ]
}

fn parse_chau7_render_pipeline_line(line: &str) -> Option<Chau7RenderPipelineSample> {
    if !line.contains("Render pipeline (30s):") {
        return None;
    }
    let timestamp = extract_chau7_timestamp(line)?;
    Some(Chau7RenderPipelineSample {
        timestamp,
        live_views: parse_key_u32(line, "liveViews=")?,
        polls: parse_key_u32(line, "polls=")?,
        changed: parse_key_u32(line, "changed=")?,
        draws: parse_key_u32(line, "draws=")?,
        sync_calls: parse_key_u32(line, "syncCalls=")?,
        sync_mib: parse_key_f32_with_suffix(line, "sync=", "MiB")?,
        full_refresh: parse_key_u32(line, "fullRefresh=").unwrap_or(0),
        physical_mb: parse_key_u32_with_suffix(line, "phys=", "MB").unwrap_or(0),
        peak_mb: parse_key_u32_with_suffix(line, "peak=", "MB").unwrap_or(0),
    })
}

fn parse_chau7_render_live_views_line(line: &str) -> Vec<Chau7RenderViewSample> {
    let Some(timestamp) = extract_chau7_timestamp(line) else {
        return Vec::new();
    };
    let Some((_, payload)) = line.split_once("Render live views (30s): ") else {
        return Vec::new();
    };

    payload
        .split(" | ")
        .filter_map(|segment| {
            Some(Chau7RenderViewSample {
                timestamp: timestamp.clone(),
                view_id: parse_key_token(segment, "view=")?,
                state: parse_key_token(segment, "state=").unwrap_or_default(),
                tab: parse_key_token(segment, "tab=").unwrap_or_default(),
                session: parse_key_token(segment, "session=").unwrap_or_default(),
                mode: parse_key_token(segment, "mode=").unwrap_or_default(),
                reasons: parse_key_token(segment, "reasons=")
                    .unwrap_or_default()
                    .split(',')
                    .filter(|reason| !reason.is_empty())
                    .map(str::to_owned)
                    .collect(),
                polls: parse_key_u32(segment, "polls=").unwrap_or(0),
                changed: parse_key_u32(segment, "changed=").unwrap_or(0),
                draws: parse_key_u32(segment, "draws=").unwrap_or(0),
                sync_calls: parse_key_u32(segment, "syncCalls=").unwrap_or(0),
                sync_mib: parse_key_f32_with_suffix(segment, "sync=", "MiB").unwrap_or(0.0),
            })
        })
        .collect()
}

fn extract_chau7_timestamp(line: &str) -> Option<String> {
    let start = line.find(" 20")? + 1;
    let rest = &line[start..];
    let end = rest.find("Z ")?;
    Some(format!("{}Z", &rest[..end]))
}

fn parse_key_token(input: &str, key: &str) -> Option<String> {
    let start = input.find(key)? + key.len();
    let rest = &input[start..];
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    Some(rest[..end].trim_end_matches('|').to_owned())
}

fn parse_key_u32(input: &str, key: &str) -> Option<u32> {
    parse_key_token(input, key)?.parse().ok()
}

fn parse_key_u32_with_suffix(input: &str, key: &str, suffix: &str) -> Option<u32> {
    parse_key_token(input, key)?
        .trim_end_matches(suffix)
        .parse()
        .ok()
}

fn parse_key_f32_with_suffix(input: &str, key: &str, suffix: &str) -> Option<f32> {
    parse_key_token(input, key)?
        .trim_end_matches(suffix)
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_sample_frames_identifies_metal_render_loop() {
        let frames = vec![
            "-[MTKView draw]".to_owned(),
            "@objc RustMetalDisplayCoordinator.draw(in:)".to_owned(),
            "MetalTerminalRenderer.render(buffer:rows:cols:dirtyRows:fullRefresh:to:viewportSize:onCompleted:)".to_owned(),
        ];

        assert_eq!(classify_sample_frames(&frames), "metal-render-loop");
    }

    #[test]
    fn parse_chau7_render_pipeline_line_extracts_recent_window() {
        let line = "[Chau7][INFO] 2026-06-25T13:20:10.545Z Render pipeline (30s): liveViews=1 ids=29 polls=1179 changed=937 draws=434 syncCalls=434 sync=282.5MiB mismatches=0 commits=434 commit=8.0MiB fullRefresh=0 maxDirtyRows=79 maxDirtyCells=10665 maxFrameCells=10665 maxInstanceBuffer=1.0MiB saturatedFrames=0 glyphCache=437 ligatureCache=4061 glyphLookups=3199500 missRate=0.0% phys=859MB peak=923MB delta=-45MB";

        let sample = match parse_chau7_render_pipeline_line(line) {
            Some(sample) => sample,
            None => panic!("render pipeline line did not parse"),
        };

        assert_eq!(sample.timestamp, "2026-06-25T13:20:10.545Z");
        assert_eq!(sample.live_views, 1);
        assert_eq!(sample.polls, 1179);
        assert_eq!(sample.draws, 434);
        assert_eq!(sample.sync_mib, 282.5);
        assert_eq!(sample.physical_mb, 859);
        assert_eq!(sample.peak_mb, 923);
    }

    #[test]
    fn parse_chau7_render_live_views_line_extracts_view_state() {
        let line = "[Chau7][INFO] 2026-06-25T13:20:10.545Z Render live views (30s): view=12 state=inactive tab=tab_12 session=019d mode=event_drain reasons=selected,notifyUpdateChanges,visible polls=87 changed=66 draws=28 syncCalls=28 sync=18.2MiB | view=29 state=active tab=tab_32 session=019e mode=event_drain:active reasons=visible-noninteractive,notifyUpdateChanges,visible polls=1119 changed=877 draws=434 syncCalls=434 sync=282.5MiB";

        let views = parse_chau7_render_live_views_line(line);

        assert_eq!(views.len(), 2);
        assert_eq!(views[0].view_id, "12");
        assert_eq!(views[0].state, "inactive");
        assert_eq!(views[0].draws, 28);
        assert_eq!(views[1].view_id, "29");
        assert_eq!(views[1].mode, "event_drain:active");
        assert!(
            views[1]
                .reasons
                .contains(&"visible-noninteractive".to_owned())
        );
        assert_eq!(views[1].sync_mib, 282.5);
    }
}
