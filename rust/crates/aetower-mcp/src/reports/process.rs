//! Process-domain report builders for the MCP server.

use std::collections::BTreeMap;

use aetower_model::SystemSnapshot;
use aetower_model::classify_signature;
use serde_json::Value;

use crate::*;

pub(crate) fn build_process_tree_report(
    snapshot: &SystemSnapshot,
    entity_id: &str,
) -> Result<ProcessTreeReport, String> {
    let root = snapshot
        .entities
        .iter()
        .find(|entity| entity.entity_id == entity_id)
        .ok_or_else(|| format!("Unknown entity_id: {entity_id}"))?;
    let seed_entities = vec![root.clone()];
    let (expanded_entities, grouping_reasons) =
        related_entities_for_process_tree(&seed_entities, &snapshot.entities);
    let expanded_ids = expanded_entities
        .iter()
        .map(|entity| entity.entity_id.clone())
        .collect::<Vec<_>>();
    let grouped_process_count = seed_entities
        .iter()
        .flat_map(|entity| entity.components.iter())
        .filter(|component| component.kind != aetower_model::ComponentKind::AdapterContext)
        .count() as u32;
    let expanded_process_count = expanded_entities
        .iter()
        .flat_map(|entity| entity.components.iter())
        .filter(|component| component.kind != aetower_model::ComponentKind::AdapterContext)
        .count() as u32;
    let roots = process_tree_roots(root, &expanded_entities);
    Ok(ProcessTreeReport {
        captured_at_millis: snapshot.captured_at_millis,
        root_entity_id: root.entity_id.clone(),
        root_display_name: root.display_name.clone(),
        seed_entity_ids: vec![root.entity_id.clone()],
        expanded_entity_ids: expanded_ids,
        grouped_process_count,
        expanded_process_count,
        grouping_reasons,
        roots,
    })
}

pub fn entity_process_tree_json(
    data_source: &dyn AetowerMcpDataSource,
    entity_id: &str,
) -> Result<String, String> {
    let snapshot = data_source.latest_snapshot()?;
    let report = build_process_tree_report(&snapshot, entity_id)?;
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

#[derive(Clone)]
pub(crate) struct RelatedProcessComponent {
    entity_id: String,
    owner_display_name: String,
    component: aetower_model::ComponentSnapshot,
    badges: Vec<String>,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct ProcessAggregate {
    subtree_cpu_percent: f32,
    subtree_memory_bytes: u64,
    subtree_process_count: u32,
}

pub(crate) fn process_tree_roots(
    root_entity: &aetower_model::EntitySnapshot,
    expanded_entities: &[aetower_model::EntitySnapshot],
) -> Vec<ProcessTreeNodeReport> {
    let related_components = expanded_entities
        .iter()
        .flat_map(|entity| {
            entity
                .components
                .iter()
                .cloned()
                .map(|component| RelatedProcessComponent {
                    entity_id: entity.entity_id.clone(),
                    owner_display_name: entity.display_name.clone(),
                    component,
                    badges: entity.badges.clone(),
                })
        })
        .collect::<Vec<_>>();
    let process_components = related_components
        .iter()
        .filter(|related| related.component.kind != aetower_model::ComponentKind::AdapterContext)
        .cloned()
        .collect::<Vec<_>>();
    let adapter_components = root_entity
        .components
        .iter()
        .filter(|component| component.kind == aetower_model::ComponentKind::AdapterContext)
        .cloned()
        .collect::<Vec<_>>();
    let pid_map = process_components
        .iter()
        .filter_map(|related| {
            related
                .component
                .process_id
                .map(|pid| (pid, related.clone()))
        })
        .collect::<BTreeMap<_, _>>();

    let mut roots = Vec::new();
    let mut children: BTreeMap<u32, Vec<RelatedProcessComponent>> = BTreeMap::new();
    for related in process_components {
        let parent_pid = extract_parent_pid(related.component.parent_summary.as_deref());
        if let Some(parent_pid) = parent_pid
            && pid_map.contains_key(&parent_pid)
        {
            children.entry(parent_pid).or_default().push(related);
        } else {
            roots.push(related);
        }
    }

    let mut reports = Vec::new();
    let total_aggregate = roots
        .iter()
        .fold(ProcessAggregate::default(), |aggregate, root| {
            let next = subtree_aggregate(root, &children);
            ProcessAggregate {
                subtree_cpu_percent: aggregate.subtree_cpu_percent + next.subtree_cpu_percent,
                subtree_memory_bytes: aggregate.subtree_memory_bytes + next.subtree_memory_bytes,
                subtree_process_count: aggregate.subtree_process_count + next.subtree_process_count,
            }
        });

    let mut sorted_roots = roots;
    sorted_roots.sort_by(|left, right| {
        let left_aggregate = subtree_aggregate(left, &children);
        let right_aggregate = subtree_aggregate(right, &children);
        right_aggregate
            .subtree_cpu_percent
            .partial_cmp(&left_aggregate.subtree_cpu_percent)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                right_aggregate
                    .subtree_memory_bytes
                    .cmp(&left_aggregate.subtree_memory_bytes)
            })
    });

    let chau7_sessions = adapter_components
        .iter()
        .filter(|component| {
            component.adapter_context.as_ref().is_some_and(|context| {
                context.kind == aetower_model::AdapterContextKind::Chau7Session
            })
        })
        .collect::<Vec<_>>();
    if !chau7_sessions.is_empty() {
        for session in chau7_sessions {
            reports.push(ProcessTreeNodeReport {
                title: session.title.clone(),
                pid: session.process_id,
                relation: "adapter-root".to_owned(),
                owner_entity_id: root_entity.entity_id.clone(),
                owner_display_name: root_entity.display_name.clone(),
                self_cpu_percent: 0.0,
                subtree_cpu_percent: total_aggregate.subtree_cpu_percent,
                self_memory_bytes: 0,
                subtree_memory_bytes: total_aggregate.subtree_memory_bytes,
                subtree_process_count: total_aggregate.subtree_process_count,
                badges: root_entity.badges.clone(),
                user: session.user.clone(),
                cwd: session
                    .adapter_context
                    .as_ref()
                    .and_then(|context| {
                        context.repo_root.clone().or(context.workspace_path.clone())
                    })
                    .or_else(|| session.cwd.clone()),
                provenance: None,
                launched_by: None,
                adapter_label: adapter_label(session),
                status_label: session
                    .adapter_context
                    .as_ref()
                    .and_then(|context| context.status.clone()),
                children: sorted_roots
                    .iter()
                    .map(|root| process_tree_node(root, &children, "session-child"))
                    .collect(),
            });
        }
    } else {
        for root in &sorted_roots {
            reports.push(process_tree_node(root, &children, "process-root"));
        }
    }

    for component in adapter_components.into_iter().filter(|component| {
        component
            .adapter_context
            .as_ref()
            .is_none_or(|context| context.kind != aetower_model::AdapterContextKind::Chau7Session)
    }) {
        reports.push(ProcessTreeNodeReport {
            title: component.title.clone(),
            pid: component.process_id,
            relation: "adapter".to_owned(),
            owner_entity_id: root_entity.entity_id.clone(),
            owner_display_name: root_entity.display_name.clone(),
            self_cpu_percent: 0.0,
            subtree_cpu_percent: 0.0,
            self_memory_bytes: 0,
            subtree_memory_bytes: 0,
            subtree_process_count: 0,
            badges: root_entity.badges.clone(),
            user: component.user.clone(),
            cwd: component
                .adapter_context
                .as_ref()
                .and_then(|context| context.repo_root.clone().or(context.workspace_path.clone()))
                .or_else(|| component.cwd.clone()),
            provenance: None,
            launched_by: None,
            adapter_label: adapter_label(&component),
            status_label: component
                .adapter_context
                .as_ref()
                .and_then(|context| context.status.clone()),
            children: Vec::new(),
        });
    }

    reports
}

pub(crate) fn process_tree_node(
    related: &RelatedProcessComponent,
    children: &BTreeMap<u32, Vec<RelatedProcessComponent>>,
    relation: &str,
) -> ProcessTreeNodeReport {
    let aggregate = subtree_aggregate(related, children);
    let child_nodes = related
        .component
        .process_id
        .and_then(|pid| children.get(&pid))
        .map(|child_components| {
            let mut child_components = child_components.clone();
            child_components.sort_by(|left, right| {
                let left_aggregate = subtree_aggregate(left, children);
                let right_aggregate = subtree_aggregate(right, children);
                right_aggregate
                    .subtree_cpu_percent
                    .partial_cmp(&left_aggregate.subtree_cpu_percent)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            child_components
                .iter()
                .map(|child| process_tree_node(child, children, "child"))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    ProcessTreeNodeReport {
        title: related.component.title.clone(),
        pid: related.component.process_id,
        relation: relation.to_owned(),
        owner_entity_id: related.entity_id.clone(),
        owner_display_name: related.owner_display_name.clone(),
        self_cpu_percent: related.component.cpu_percent,
        subtree_cpu_percent: aggregate.subtree_cpu_percent,
        self_memory_bytes: related.component.memory_bytes,
        subtree_memory_bytes: aggregate.subtree_memory_bytes,
        subtree_process_count: aggregate.subtree_process_count,
        badges: related.badges.clone(),
        user: related.component.user.clone(),
        cwd: related.component.cwd.clone(),
        provenance: related
            .component
            .provenance
            .as_ref()
            .map(|provenance| format!("{:?}: {}", provenance.kind, provenance.label)),
        launched_by: related.component.launched_by.clone(),
        adapter_label: None,
        status_label: if aggregate.subtree_process_count > 1 {
            Some("group".to_owned())
        } else {
            None
        },
        children: child_nodes,
    }
}

pub(crate) fn subtree_aggregate(
    related: &RelatedProcessComponent,
    children: &BTreeMap<u32, Vec<RelatedProcessComponent>>,
) -> ProcessAggregate {
    let mut aggregate = ProcessAggregate {
        subtree_cpu_percent: related.component.cpu_percent,
        subtree_memory_bytes: related.component.memory_bytes,
        subtree_process_count: 1,
    };
    if let Some(pid) = related.component.process_id
        && let Some(child_components) = children.get(&pid)
    {
        for child in child_components {
            let child_aggregate = subtree_aggregate(child, children);
            aggregate.subtree_cpu_percent += child_aggregate.subtree_cpu_percent;
            aggregate.subtree_memory_bytes += child_aggregate.subtree_memory_bytes;
            aggregate.subtree_process_count += child_aggregate.subtree_process_count;
        }
    }
    aggregate
}

pub(crate) fn related_entities_for_process_tree(
    seed_entities: &[aetower_model::EntitySnapshot],
    all_entities: &[aetower_model::EntitySnapshot],
) -> (Vec<aetower_model::EntitySnapshot>, Vec<String>) {
    let mut included_ids = seed_entities
        .iter()
        .map(|entity| entity.entity_id.clone())
        .collect::<BTreeSet<_>>();
    let mut included_pids = seed_entities
        .iter()
        .flat_map(|entity| {
            entity
                .components
                .iter()
                .filter_map(|component| component.process_id)
        })
        .collect::<BTreeSet<_>>();
    let selected_session_ids = seed_entities
        .iter()
        .flat_map(entity_session_ids)
        .collect::<BTreeSet<_>>();
    let selected_repo_roots = seed_entities
        .iter()
        .flat_map(entity_repo_roots)
        .collect::<BTreeSet<_>>();
    let mut reasons = BTreeSet::new();
    let mut changed = true;
    while changed {
        changed = false;
        let candidates = all_entities
            .iter()
            .filter(|entity| !included_ids.contains(&entity.entity_id))
            .cloned()
            .collect::<Vec<_>>();
        for candidate in candidates {
            let is_child_by_pid = candidate.components.iter().any(|component| {
                extract_parent_pid(component.parent_summary.as_deref())
                    .is_some_and(|parent_pid| included_pids.contains(&parent_pid))
            });
            let shares_chau7_context = candidate.badges.iter().any(|badge| badge == "chau7-live")
                && (!selected_session_ids.is_disjoint(&entity_session_ids(&candidate))
                    || candidate.components.iter().any(|component| {
                        [
                            component.cwd.as_deref(),
                            component.executable_path.as_deref(),
                            component
                                .adapter_context
                                .as_ref()
                                .and_then(|context| context.workspace_path.as_deref()),
                            component
                                .adapter_context
                                .as_ref()
                                .and_then(|context| context.repo_root.as_deref()),
                        ]
                        .into_iter()
                        .flatten()
                        .any(|path| {
                            selected_repo_roots
                                .iter()
                                .any(|root| path.starts_with(root))
                        })
                    }));
            if is_child_by_pid || shares_chau7_context {
                included_ids.insert(candidate.entity_id.clone());
                included_pids.extend(
                    candidate
                        .components
                        .iter()
                        .filter_map(|component| component.process_id),
                );
                if is_child_by_pid {
                    reasons.insert("expanded through parent/child PID lineage".to_owned());
                }
                if shares_chau7_context {
                    reasons.insert(
                        "expanded through shared Chau7 session or workspace context".to_owned(),
                    );
                }
                changed = true;
            }
        }
    }
    (
        all_entities
            .iter()
            .filter(|entity| included_ids.contains(&entity.entity_id))
            .cloned()
            .collect(),
        reasons.into_iter().collect(),
    )
}

pub(crate) fn extract_parent_pid(summary: Option<&str>) -> Option<u32> {
    let summary = summary?;
    let marker = summary.find("pid ")?;
    let digits = summary[marker + 4..]
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();
    digits.parse::<u32>().ok()
}

pub(crate) fn adapter_label(component: &aetower_model::ComponentSnapshot) -> Option<String> {
    match component
        .adapter_context
        .as_ref()
        .map(|context| &context.kind)
    {
        Some(aetower_model::AdapterContextKind::Chau7Session) => Some("chau7".to_owned()),
        Some(aetower_model::AdapterContextKind::ChromiumTab) => Some("chromium".to_owned()),
        Some(aetower_model::AdapterContextKind::DockerContainer) => Some("docker".to_owned()),
        Some(aetower_model::AdapterContextKind::PrivilegedSocket) => Some("helper".to_owned()),
        Some(aetower_model::AdapterContextKind::VsCodeWorkspace)
        | Some(aetower_model::AdapterContextKind::VsCodeRuntime) => Some("vscode".to_owned()),
        Some(aetower_model::AdapterContextKind::Unknown) | None => None,
    }
}

pub(crate) fn process_dynamic_tool_request(
    data_source: &dyn AetowerMcpDataSource,
    request: &DynamicToolRequest,
) -> Result<Value, String> {
    match request {
        DynamicToolRequest::MemoryBreakdown {
            entity_id,
            top_regions,
        } => tool_json(build_entity_memory_breakdown(
            data_source,
            entity_id,
            *top_regions,
        )?)
        .map_err(|error| extract_tool_error_message(&error)),
        DynamicToolRequest::ProfileEntity {
            entity_id,
            duration_seconds,
            top_stacks,
        } => tool_json(build_entity_profile(
            data_source,
            entity_id,
            *duration_seconds,
            *top_stacks,
        )?)
        .map_err(|error| extract_tool_error_message(&error)),
        DynamicToolRequest::WakeupAttribution {
            entity_id,
            duration_seconds,
            top_stacks,
        } => tool_json(build_wakeup_attribution(
            data_source,
            entity_id,
            *duration_seconds,
            *top_stacks,
        )?)
        .map_err(|error| extract_tool_error_message(&error)),
        DynamicToolRequest::ProcessInspect { pid } => {
            tool_json(build_process_inspection(data_source, *pid)?)
                .map_err(|error| extract_tool_error_message(&error))
        }
        DynamicToolRequest::ProcessOpenResources { pid, limit } => {
            tool_json(build_process_open_resources(*pid, *limit)?)
                .map_err(|error| extract_tool_error_message(&error))
        }
        DynamicToolRequest::ProcessSample {
            pid,
            duration_seconds,
            top_stacks,
        } => tool_json(build_process_sample(
            *pid,
            (*duration_seconds).clamp(1, MAX_PROFILE_DURATION_SECONDS),
            *top_stacks,
        )?)
        .map_err(|error| extract_tool_error_message(&error)),
        DynamicToolRequest::ProcessAction {
            pid,
            action,
            dry_run,
            reason,
        } => tool_json(build_process_action(
            data_source,
            *pid,
            action,
            *dry_run,
            reason.clone(),
        )?)
        .map_err(|error| extract_tool_error_message(&error)),
        DynamicToolRequest::ProcessActionHistory {
            window_minutes,
            limit,
        } => tool_json(build_process_action_history(
            data_source,
            *window_minutes,
            *limit,
        )?)
        .map_err(|error| extract_tool_error_message(&error)),
    }
}

pub(crate) fn build_entity_memory_breakdown(
    data_source: &dyn AetowerMcpDataSource,
    entity_id: &str,
    top_regions: usize,
) -> Result<EntityMemoryBreakdown, String> {
    let snapshot = data_source.latest_snapshot()?;
    let entity = snapshot
        .entities
        .iter()
        .find(|entity| entity.entity_id == entity_id)
        .ok_or_else(|| format!("Unknown entity_id: {entity_id}"))?;
    let process_ids = entity_process_ids(entity);
    if process_ids.is_empty() {
        return Err(format!(
            "Entity {} has no attributed process IDs to inspect.",
            entity.display_name
        ));
    }
    let regions = vmmap_regions_for_processes(&process_ids, top_regions.max(1))?;
    Ok(EntityMemoryBreakdown {
        captured_at_millis: snapshot.captured_at_millis,
        entity_id: entity.entity_id.clone(),
        display_name: entity.display_name.clone(),
        process_ids,
        resident_bytes: entity.metrics.memory_resident_bytes,
        physical_footprint_bytes: entity.metrics.memory_physical_footprint_bytes,
        memory_metric_note: if entity.metrics.memory_physical_footprint_bytes > 0 {
            "resident_bytes is the current resident set; physical_footprint_bytes follows macOS task footprint when available and may exceed resident because it includes graphics and other charged memory.".to_owned()
        } else {
            "resident_bytes is the current resident set; physical_footprint_bytes is unavailable for this entity on the current platform or sample.".to_owned()
        },
        regions,
    })
}

pub(crate) fn vmmap_regions_for_processes(
    process_ids: &[u32],
    top_regions: usize,
) -> Result<Vec<MemoryRegionBreakdown>, String> {
    let mut regions_by_type = BTreeMap::<String, MemoryRegionBreakdown>::new();
    for pid in process_ids {
        let output = run_os_command("/usr/bin/vmmap", &[pid.to_string()])?;
        for region in parse_vmmap_regions(&output) {
            let entry = regions_by_type.entry(region.region_type.clone()).or_insert(
                MemoryRegionBreakdown {
                    region_type: region.region_type.clone(),
                    virtual_bytes: 0,
                    resident_bytes: 0,
                    dirty_bytes: 0,
                    swap_bytes: 0,
                },
            );
            entry.virtual_bytes += region.virtual_bytes;
            entry.resident_bytes += region.resident_bytes;
            entry.dirty_bytes += region.dirty_bytes;
            entry.swap_bytes += region.swap_bytes;
        }
    }
    let mut regions = regions_by_type.into_values().collect::<Vec<_>>();
    regions.sort_by(|left, right| {
        right
            .resident_bytes
            .cmp(&left.resident_bytes)
            .then_with(|| right.virtual_bytes.cmp(&left.virtual_bytes))
    });
    regions.truncate(top_regions.max(1));
    Ok(regions)
}

pub fn memory_breakdown_json(
    data_source: &dyn AetowerMcpDataSource,
    entity_id: &str,
    top_regions: usize,
) -> Result<String, String> {
    let report = build_entity_memory_breakdown(data_source, entity_id, top_regions.max(1))?;
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

pub fn self_memory_attribution_json(
    data_source: &dyn AetowerMcpDataSource,
    top_regions: usize,
) -> Result<String, String> {
    let runtime = data_source.latest_runtime_lag_metrics()?;
    let samples = vec![self_runtime_watch_sample(&runtime)];
    let report =
        build_self_runtime_memory_attribution(data_source, &samples, true, top_regions.max(1));
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

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
        queue_breakdown,
        dominant_cause,
        attribution_mode: "sampled-call-stack-heuristic".to_owned(),
        caveats: vec![
            "This is a sampled heuristic based on `sample`, not exact kernel wakeup accounting."
                .to_owned(),
            "Queue labels are only present when the sampled stack exposed one.".to_owned(),
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

#[derive(Debug, Clone)]
pub(crate) struct ProcessComponentContext {
    entity_id: String,
    display_name: String,
    component_title: String,
    component_kind: String,
    executable_path: Option<String>,
    command_line: Option<String>,
    cwd: Option<String>,
    user: Option<String>,
    parent_summary: Option<String>,
    cpu_percent: f32,
    memory_bytes: u64,
    memory_physical_footprint_bytes: u64,
    start_time_millis: u64,
    sibling_process_count: u32,
}

pub(crate) fn build_process_inspection(
    data_source: &dyn AetowerMcpDataSource,
    pid: u32,
) -> Result<ProcessInspectionReport, String> {
    validate_pid(pid)?;
    let snapshot = data_source.latest_snapshot()?;
    let context = process_component_context(&snapshot, pid);
    let ps = process_ps_summary(pid).ok();
    let alive = ps.is_some() || process_exists(pid);
    let child_pids = process_child_pids(&snapshot, pid);
    let parent_pid = context
        .as_ref()
        .and_then(|context| extract_parent_pid(context.parent_summary.as_deref()))
        .or_else(|| ps.as_ref().and_then(|summary| summary.parent_pid));
    let mut safety_notes = Vec::new();
    if pid == std::process::id() {
        safety_notes.push(
            "This is the running Aetower process; destructive actions are blocked.".to_owned(),
        );
    }
    if context.is_none() {
        safety_notes.push(
            "This PID is not currently attributed to an Aetower entity; live ps data may still exist."
                .to_owned(),
        );
    }
    if !alive {
        safety_notes.push("The process is not visible to ps right now.".to_owned());
    }

    let executable_path = context
        .as_ref()
        .and_then(|context| context.executable_path.clone());
    let bundle = executable_path
        .as_deref()
        .and_then(read_process_bundle_info);
    let signature = executable_path.as_deref().map(read_process_signature);
    let entitlements = executable_path
        .as_deref()
        .map(read_process_entitlements)
        .unwrap_or_default();
    let (environment, environment_note) = read_process_environment(pid);
    let startup_entry = executable_path.as_deref().and_then(lookup_startup_entry);
    let dyld_insert = environment
        .iter()
        .find(|entry| entry.key == "DYLD_INSERT_LIBRARIES")
        .map(|entry| entry.value.clone())
        .unwrap_or_default();
    let (loaded_dylibs, dylib_summary) = read_process_dylibs(pid, &dyld_insert);

    Ok(ProcessInspectionReport {
        captured_at_millis: snapshot.captured_at_millis,
        pid,
        alive,
        entity_id: context.as_ref().map(|context| context.entity_id.clone()),
        display_name: context.as_ref().map(|context| context.display_name.clone()),
        component_title: context
            .as_ref()
            .map(|context| context.component_title.clone()),
        component_kind: context
            .as_ref()
            .map(|context| context.component_kind.clone()),
        executable_path,
        command_line: context
            .as_ref()
            .and_then(|context| context.command_line.clone())
            .or_else(|| ps.as_ref().and_then(|summary| summary.command.clone())),
        cwd: context.as_ref().and_then(|context| context.cwd.clone()),
        user: context
            .as_ref()
            .and_then(|context| context.user.clone())
            .or_else(|| ps.as_ref().and_then(|summary| summary.user.clone())),
        parent_pid,
        parent_summary: context
            .as_ref()
            .and_then(|context| context.parent_summary.clone()),
        cpu_percent: context
            .as_ref()
            .map(|context| context.cpu_percent)
            .or_else(|| ps.as_ref().and_then(|summary| summary.cpu_percent)),
        memory_bytes: context
            .as_ref()
            .map(|context| context.memory_bytes)
            .or_else(|| ps.as_ref().and_then(|summary| summary.resident_bytes)),
        memory_physical_footprint_bytes: context
            .as_ref()
            .map(|context| context.memory_physical_footprint_bytes),
        start_time_millis: context.as_ref().map(|context| context.start_time_millis),
        child_pids,
        sibling_process_count: context
            .as_ref()
            .map(|context| context.sibling_process_count)
            .unwrap_or(0),
        ps,
        bundle,
        signature,
        entitlements,
        environment,
        environment_note,
        startup_entry,
        loaded_dylibs,
        dylib_summary,
        safety_notes,
    })
}

pub fn process_inspect_json(
    data_source: &dyn AetowerMcpDataSource,
    pid: u32,
) -> Result<String, String> {
    let report = build_process_inspection(data_source, pid)?;
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

pub(crate) fn build_process_open_resources(
    pid: u32,
    limit: usize,
) -> Result<ProcessOpenResourcesReport, String> {
    validate_pid(pid)?;
    let output = run_os_command(
        "/usr/sbin/lsof",
        &["-nP".to_owned(), "-p".to_owned(), pid.to_string()],
    )?;
    let mut resources = parse_lsof_resources(&output);
    let resource_count = resources.len();
    let socket_count = resources
        .iter()
        .filter(|resource| resource.is_socket)
        .count();
    let file_count = resource_count.saturating_sub(socket_count);
    resources.truncate(limit.max(1));
    Ok(ProcessOpenResourcesReport {
        captured_at_millis: current_unix_millis().unwrap_or_default(),
        pid,
        resource_count,
        returned: resources.len(),
        file_count,
        socket_count,
        resources,
    })
}

pub fn process_open_resources_json(pid: u32, limit: usize) -> Result<String, String> {
    let report = build_process_open_resources(pid, limit)?;
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

pub(crate) fn build_process_action(
    data_source: &dyn AetowerMcpDataSource,
    pid: u32,
    action: &str,
    dry_run: bool,
    reason: Option<String>,
) -> Result<ProcessActionReport, String> {
    validate_pid(pid)?;
    let snapshot = data_source.latest_snapshot().ok();
    let plan = process_action_plan(snapshot.as_ref(), pid, action)?;
    let context = snapshot
        .as_ref()
        .and_then(|snapshot| process_component_context(snapshot, pid));
    let mut safety_notes = Vec::new();
    if plan.target_pids.contains(&std::process::id()) {
        safety_notes.push("Aetower refuses to target its own running process.".to_owned());
        if !dry_run {
            return Err("Refusing to target the running Aetower process.".to_owned());
        }
    }
    if !dry_run {
        let missing_pids = plan
            .target_pids
            .iter()
            .copied()
            .filter(|target_pid| !process_exists(*target_pid))
            .collect::<Vec<_>>();
        if !missing_pids.is_empty() {
            return Err(format!(
                "Target process(es) are not visible to ps right now: {missing_pids:?}."
            ));
        }
    }

    if dry_run {
        return Ok(ProcessActionReport {
            captured_at_millis: current_unix_millis().unwrap_or_default(),
            pid,
            target_pids: plan.target_pids,
            action: plan.normalized_action,
            signal: plan.signal,
            dry_run,
            executed: false,
            success: true,
            command: plan.command,
            reason,
            entity_id: context.as_ref().map(|context| context.entity_id.clone()),
            display_name: context.as_ref().map(|context| context.display_name.clone()),
            message: plan.dry_run_message,
            safety_notes,
        });
    }

    let status = Command::new(&plan.program)
        .args(&plan.args)
        .status()
        .map_err(|error| format!("run {}: {error}", plan.program))?;
    let success = status.success();
    let message = if success {
        plan.success_message.clone()
    } else {
        format!("{} exited with status {status}.", plan.program)
    };
    let report = ProcessActionReport {
        captured_at_millis: current_unix_millis().unwrap_or_default(),
        pid,
        target_pids: plan.target_pids,
        action: plan.normalized_action,
        signal: plan.signal,
        dry_run,
        executed: true,
        success,
        command: plan.command,
        reason: reason.clone(),
        entity_id: context.as_ref().map(|context| context.entity_id.clone()),
        display_name: context.as_ref().map(|context| context.display_name.clone()),
        message,
        safety_notes,
    };
    data_source.record_diagnostics_event(process_action_diagnostics_event(&report));
    Ok(report)
}

pub fn process_action_json(
    data_source: &dyn AetowerMcpDataSource,
    pid: u32,
    action: &str,
    dry_run: bool,
    reason: Option<String>,
) -> Result<String, String> {
    let report = build_process_action(data_source, pid, action, dry_run, reason)?;
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

pub(crate) fn build_process_action_history(
    data_source: &dyn AetowerMcpDataSource,
    window_minutes: u64,
    limit: usize,
) -> Result<ProcessActionHistoryReport, String> {
    let now = current_unix_millis().unwrap_or_default();
    let events = data_source.query_diagnostics(DiagnosticsQuery {
        limit: limit.saturating_mul(4).max(limit).max(1),
        search: Some("process-action".to_owned()),
        since_millis: Some(now.saturating_sub(window_minutes.saturating_mul(60 * 1000))),
        include_persisted: true,
        ..DiagnosticsQuery::default()
    })?;
    let actions = events
        .into_iter()
        .filter(|event| event.event_type == "process-action")
        .take(limit.max(1))
        .map(process_action_history_item)
        .collect::<Vec<_>>();
    Ok(ProcessActionHistoryReport {
        window_minutes,
        returned: actions.len(),
        actions,
    })
}

pub fn process_action_history_json(
    data_source: &dyn AetowerMcpDataSource,
    window_minutes: u64,
    limit: usize,
) -> Result<String, String> {
    let report = build_process_action_history(data_source, window_minutes.max(1), limit.max(1))?;
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

pub(crate) fn entity_process_ids(entity: &aetower_model::EntitySnapshot) -> Vec<u32> {
    let mut process_ids = entity
        .components
        .iter()
        .filter_map(|component| {
            (component.kind != aetower_model::ComponentKind::AdapterContext)
                .then_some(component.process_id)
                .flatten()
        })
        .collect::<Vec<_>>();
    process_ids.sort_unstable();
    process_ids.dedup();
    process_ids
}

pub(crate) fn process_component_context(
    snapshot: &SystemSnapshot,
    pid: u32,
) -> Option<ProcessComponentContext> {
    for entity in &snapshot.entities {
        for component in &entity.components {
            if component.process_id != Some(pid) {
                continue;
            }
            let sibling_process_count = entity_process_ids(entity).len() as u32;
            return Some(ProcessComponentContext {
                entity_id: entity.entity_id.clone(),
                display_name: entity.display_name.clone(),
                component_title: component.title.clone(),
                component_kind: format!("{:?}", component.kind),
                executable_path: component.executable_path.clone(),
                command_line: component.command_line.clone(),
                cwd: component.cwd.clone(),
                user: component.user.clone(),
                parent_summary: component.parent_summary.clone(),
                cpu_percent: component.cpu_percent,
                memory_bytes: component.memory_bytes,
                memory_physical_footprint_bytes: component.memory_physical_footprint_bytes,
                start_time_millis: component.start_time_millis,
                sibling_process_count,
            });
        }
    }
    None
}

pub(crate) fn process_child_pids(snapshot: &SystemSnapshot, pid: u32) -> Vec<u32> {
    let mut child_pids = snapshot
        .entities
        .iter()
        .flat_map(|entity| &entity.components)
        .filter(|component| extract_parent_pid(component.parent_summary.as_deref()) == Some(pid))
        .filter_map(|component| component.process_id)
        .collect::<Vec<_>>();
    child_pids.sort_unstable();
    child_pids.dedup();
    child_pids
}

pub(crate) fn validate_pid(pid: u32) -> Result<(), String> {
    if pid <= 1 {
        Err(format!(
            "Refusing to inspect or signal protected pid {pid}."
        ))
    } else {
        Ok(())
    }
}

pub(crate) fn process_exists(pid: u32) -> bool {
    Command::new("/bin/ps")
        .arg("-p")
        .arg(pid.to_string())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub(crate) fn process_ps_summary(pid: u32) -> Result<ProcessPsSummary, String> {
    let output = run_os_command(
        "/bin/ps",
        &[
            "-p".to_owned(),
            pid.to_string(),
            "-o".to_owned(),
            "ppid=".to_owned(),
            "-o".to_owned(),
            "user=".to_owned(),
            "-o".to_owned(),
            "stat=".to_owned(),
            "-o".to_owned(),
            "pcpu=".to_owned(),
            "-o".to_owned(),
            "rss=".to_owned(),
            "-o".to_owned(),
            "command=".to_owned(),
        ],
    )?;
    parse_ps_summary(&output)
}

pub(crate) fn read_process_bundle_info(executable_path: &str) -> Option<ProcessBundleInfo> {
    let bundle_path = containing_app_bundle(executable_path)?;
    let info_plist = bundle_path.join("Contents/Info.plist");
    Some(ProcessBundleInfo {
        bundle_id: plist_value(&info_plist, "CFBundleIdentifier"),
        bundle_path: Some(bundle_path.to_string_lossy().to_string()),
        name: plist_value(&info_plist, "CFBundleName")
            .or_else(|| plist_value(&info_plist, "CFBundleDisplayName")),
        short_version: plist_value(&info_plist, "CFBundleShortVersionString"),
        version: plist_value(&info_plist, "CFBundleVersion"),
    })
}

pub(crate) fn read_process_signature(executable_path: &str) -> ProcessSignatureInfo {
    let output = Command::new("/usr/bin/codesign")
        .args(["-dv", "--verbose=4", executable_path])
        .output();
    let Ok(output) = output else {
        return ProcessSignatureInfo {
            signed: false,
            signing_id: None,
            team_id: None,
            authority: Vec::new(),
            notarized: None,
            classification: "unknown".to_owned(),
            is_adhoc: false,
            note: Some("codesign could not be executed.".to_owned()),
        };
    };

    let details = String::from_utf8_lossy(&output.stderr);
    let authority = details
        .lines()
        .filter_map(|line| line.trim().strip_prefix("Authority="))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let signed = output.status.success();
    let (classification, is_adhoc) = classify_signature(signed, &authority);
    ProcessSignatureInfo {
        signed,
        signing_id: parse_codesign_field(&details, "Identifier="),
        team_id: parse_codesign_field(&details, "TeamIdentifier="),
        authority,
        notarized: signed.then(|| spctl_accepts_executable(executable_path)),
        classification,
        is_adhoc,
        note: (!signed).then(|| {
            String::from_utf8_lossy(&output.stderr)
                .lines()
                .next()
                .unwrap_or("codesign did not accept this executable.")
                .trim()
                .to_owned()
        }),
    }
}

pub(crate) fn read_process_entitlements(executable_path: &str) -> Vec<String> {
    let output = Command::new("/usr/bin/codesign")
        .args(["-d", "--entitlements", ":-", executable_path])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut entitlements = text
        .lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("<key>")
                .and_then(|value| value.strip_suffix("</key>"))
                .map(str::to_owned)
        })
        .collect::<Vec<_>>();
    entitlements.sort();
    entitlements.dedup();
    entitlements
}

/// Read a same-user process's environment via `KERN_PROCARGS2`, redacting
/// values whose key or shape looks like a secret. Other-user/system processes
/// return empty with a friendly note (no elevation in this build).
pub(crate) fn read_process_environment(pid: u32) -> (Vec<EnvVarEntry>, Option<String>) {
    const MAX_ENV: usize = 512;
    match procargs_environment(pid) {
        Ok(pairs) => {
            let truncated = pairs.len() > MAX_ENV;
            let mut redacted_count = 0usize;
            let entries = pairs
                .into_iter()
                .take(MAX_ENV)
                .map(|(key, value)| {
                    if value_is_secret(&key, &value) {
                        redacted_count += 1;
                        EnvVarEntry {
                            key,
                            value: format!("‹redacted · {} chars›", value.chars().count()),
                        }
                    } else {
                        EnvVarEntry { key, value }
                    }
                })
                .collect::<Vec<_>>();
            let mut notes = Vec::new();
            if truncated {
                notes.push(format!("showing the first {MAX_ENV}"));
            }
            if redacted_count > 0 {
                notes.push(format!(
                    "{redacted_count} value(s) redacted as likely secrets"
                ));
            }
            let note = if entries.is_empty() {
                Some("No environment variables were readable for this process.".to_owned())
            } else if notes.is_empty() {
                None
            } else {
                Some(notes.join("; "))
            };
            (entries, note)
        }
        Err(message) => (Vec::new(), Some(message)),
    }
}

/// Heuristic secret detection: redact by key name (the common case) or by a few
/// well-known value prefixes (OpenAI `sk-`, GitHub `ghp_`, AWS `AKIA`, PEM).
fn value_is_secret(key: &str, value: &str) -> bool {
    const SECRET_KEY_MARKERS: [&str; 11] = [
        "KEY",
        "TOKEN",
        "SECRET",
        "PASSWORD",
        "PASSWD",
        "AUTH",
        "SESSION",
        "CREDENTIAL",
        "PRIVATE",
        "APIKEY",
        "ACCESS_KEY",
    ];
    let upper_key = key.to_ascii_uppercase();
    if SECRET_KEY_MARKERS
        .iter()
        .any(|marker| upper_key.contains(marker))
    {
        return true;
    }
    let trimmed = value.trim_start();
    ["sk-", "ghp_", "gho_", "github_pat_", "AKIA", "-----BEGIN"]
        .iter()
        .any(|prefix| trimmed.starts_with(prefix))
}

const CTL_KERN: libc::c_int = 1;
const KERN_ARGMAX: libc::c_int = 8;
const KERN_PROCARGS2: libc::c_int = 49;

/// Pull another process's argv+environment buffer from `KERN_PROCARGS2`. Works
/// for same-user processes without elevation; otherwise the kernel returns an
/// error we translate into a friendly message.
fn procargs_environment(pid: u32) -> Result<Vec<(String, String)>, String> {
    let mut argmax: libc::c_int = 0;
    let mut argmax_size = std::mem::size_of::<libc::c_int>();
    let mut argmax_mib = [CTL_KERN, KERN_ARGMAX];
    let argmax_rc = unsafe {
        libc::sysctl(
            argmax_mib.as_mut_ptr(),
            argmax_mib.len() as libc::c_uint,
            &mut argmax as *mut _ as *mut libc::c_void,
            &mut argmax_size,
            std::ptr::null_mut(),
            0,
        )
    };
    if argmax_rc != 0 || argmax <= 0 {
        return Err("Could not determine the process argument buffer size.".to_owned());
    }

    let mut buffer = vec![0u8; argmax as usize];
    let mut buffer_size = buffer.len();
    let mut mib = [CTL_KERN, KERN_PROCARGS2, pid as libc::c_int];
    let rc = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as libc::c_uint,
            buffer.as_mut_ptr() as *mut libc::c_void,
            &mut buffer_size,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 {
        return Err(
            "Environment variables require same-user ownership (elevation is not enabled in this build)."
                .to_owned(),
        );
    }
    buffer.truncate(buffer_size);
    Ok(parse_procargs2(&buffer))
}

/// Parse a `KERN_PROCARGS2` buffer: `[argc:i32][exec_path\0][\0..][argv\0..][env\0..]`.
pub(crate) fn parse_procargs2(buffer: &[u8]) -> Vec<(String, String)> {
    if buffer.len() < 4 {
        return Vec::new();
    }
    let argc = i32::from_ne_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]).max(0) as usize;
    let mut pos = 4;
    // Skip the executable path string.
    while pos < buffer.len() && buffer[pos] != 0 {
        pos += 1;
    }
    // Skip the NUL padding between exec path and argv.
    while pos < buffer.len() && buffer[pos] == 0 {
        pos += 1;
    }
    // Skip argc argument strings.
    let mut args_seen = 0;
    while args_seen < argc && pos < buffer.len() {
        while pos < buffer.len() && buffer[pos] != 0 {
            pos += 1;
        }
        pos += 1; // consume the NUL terminator
        args_seen += 1;
    }
    // The remainder is KEY=VALUE environment entries until an empty string.
    let mut envs = Vec::new();
    while pos < buffer.len() {
        let start = pos;
        while pos < buffer.len() && buffer[pos] != 0 {
            pos += 1;
        }
        let bytes = &buffer[start..pos];
        pos += 1; // consume the NUL terminator
        if bytes.is_empty() {
            break;
        }
        if let Ok(text) = std::str::from_utf8(bytes)
            && let Some((key, value)) = text.split_once('=')
        {
            envs.push((key.to_owned(), value.to_owned()));
        }
    }
    envs
}

pub(crate) fn lookup_startup_entry(executable_path: &str) -> Option<StartupEntryInfo> {
    for (kind, directory) in [
        ("launch-agent", "/Library/LaunchAgents"),
        ("launch-daemon", "/Library/LaunchDaemons"),
        ("user-launch-agent", "~/Library/LaunchAgents"),
    ] {
        let directory = directory.replace('~', &std::env::var("HOME").unwrap_or_default());
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("plist") {
                continue;
            }
            let Ok(contents) = std::fs::read_to_string(&path) else {
                continue;
            };
            if contents.contains(executable_path) {
                return Some(StartupEntryInfo {
                    kind: kind.to_owned(),
                    label: plist_value(&path, "Label"),
                    plist_path: path.to_string_lossy().to_string(),
                });
            }
        }
    }
    None
}

fn containing_app_bundle(executable_path: &str) -> Option<std::path::PathBuf> {
    std::path::Path::new(executable_path)
        .ancestors()
        .find(|path| path.extension().and_then(|extension| extension.to_str()) == Some("app"))
        .map(std::path::Path::to_path_buf)
}

fn plist_value(plist_path: &std::path::Path, key: &str) -> Option<String> {
    if !plist_path.exists() {
        return None;
    }
    let output = Command::new("/usr/libexec/PlistBuddy")
        .args(["-c", &format!("Print :{key}")])
        .arg(plist_path)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!value.is_empty()).then_some(value)
}

fn parse_codesign_field(details: &str, prefix: &str) -> Option<String> {
    details
        .lines()
        .find_map(|line| line.trim().strip_prefix(prefix))
        .map(str::to_owned)
}

/// Enumerate dynamic libraries mapped into a process via `lsof` (reusing the
/// open-resources command), classify each system vs third-party, and flag any
/// listed in the process's `DYLD_INSERT_LIBRARIES` as injected.
pub(crate) fn read_process_dylibs(
    pid: u32,
    dyld_insert: &str,
) -> (Vec<ProcessDylib>, DylibSummary) {
    let injected = dyld_insert
        .split(':')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .collect::<Vec<_>>();
    match run_os_command(
        "/usr/sbin/lsof",
        &["-nP".to_owned(), "-p".to_owned(), pid.to_string()],
    ) {
        Ok(output) => parse_dylibs_from_lsof(&output, &injected),
        Err(_) => (Vec::new(), DylibSummary::default()),
    }
}

pub(crate) fn parse_dylibs_from_lsof(
    output: &str,
    injected: &[&str],
) -> (Vec<ProcessDylib>, DylibSummary) {
    const MAX_DYLIBS: usize = 400;
    let mut seen = std::collections::BTreeSet::new();
    let mut dylibs = Vec::new();
    for line in output.lines().skip(1) {
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if parts.len() < 9 {
            continue;
        }
        let path = parts[8..].join(" ");
        if !(path.ends_with(".dylib") || path.contains(".framework/")) {
            continue;
        }
        if !seen.insert(path.clone()) {
            continue;
        }
        let category = if path.starts_with("/System/")
            || path.starts_with("/usr/lib/")
            || path.starts_with("/Library/Apple/")
        {
            "system"
        } else {
            "third_party"
        };
        let name = path.rsplit('/').next().unwrap_or(&path).to_owned();
        dylibs.push(ProcessDylib {
            injected: injected.iter().any(|entry| *entry == path),
            path,
            name,
            category: category.to_owned(),
        });
        if dylibs.len() >= MAX_DYLIBS {
            break;
        }
    }
    dylibs.sort_by(|left, right| {
        left.category
            .cmp(&right.category)
            .then(left.name.cmp(&right.name))
    });
    let summary = DylibSummary {
        total: dylibs.len() as u32,
        third_party: dylibs
            .iter()
            .filter(|d| d.category == "third_party")
            .count() as u32,
        injected: dylibs.iter().filter(|d| d.injected).count() as u32,
    };
    (dylibs, summary)
}

fn spctl_accepts_executable(executable_path: &str) -> bool {
    Command::new("/usr/sbin/spctl")
        .args(["--assess", "--type", "execute", executable_path])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub(crate) fn parse_ps_summary(output: &str) -> Result<ProcessPsSummary, String> {
    let line = output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or_else(|| "ps returned no process rows".to_owned())?;
    let parts = line.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 5 {
        return Err(format!("ps row had too few columns: {line}"));
    }
    Ok(ProcessPsSummary {
        parent_pid: parts[0].parse::<u32>().ok(),
        user: Some(parts[1].to_owned()),
        status: Some(parts[2].to_owned()),
        cpu_percent: parts[3].parse::<f32>().ok(),
        resident_bytes: parts[4]
            .parse::<u64>()
            .ok()
            .map(|rss_kib| rss_kib.saturating_mul(1024)),
        command: (parts.len() > 5).then(|| parts[5..].join(" ")),
    })
}

pub(crate) fn parse_lsof_resources(output: &str) -> Vec<ProcessOpenResource> {
    output
        .lines()
        .skip(1)
        .filter_map(parse_lsof_resource_line)
        .collect()
}

pub(crate) fn parse_lsof_resource_line(line: &str) -> Option<ProcessOpenResource> {
    let parts = line.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 9 {
        return None;
    }
    let resource_type = parts[4].to_owned();
    let name = parts[8..].join(" ");
    let is_socket = matches!(resource_type.as_str(), "IPv4" | "IPv6" | "unix")
        || name.contains("TCP ")
        || name.contains("UDP ")
        || name.contains("->");
    Some(ProcessOpenResource {
        fd: parts[3].to_owned(),
        resource_type,
        name,
        detail: Some(parts[5..8].join(" ")),
        is_socket,
    })
}

pub(crate) fn process_action_plan(
    snapshot: Option<&SystemSnapshot>,
    pid: u32,
    action: &str,
) -> Result<ProcessActionPlan, String> {
    match action.trim().to_ascii_lowercase().as_str() {
        "terminate" | "term" | "sigterm" => Ok(signal_process_action_plan(
            "terminate",
            "TERM",
            vec![pid],
            "Dry run only; no terminate signal was sent.",
            format!("Sent TERM to process {pid}."),
        )),
        "force-kill" | "force_kill" | "kill" | "sigkill" => Ok(signal_process_action_plan(
            "force-kill",
            "KILL",
            vec![pid],
            "Dry run only; no force-kill signal was sent.",
            format!("Sent KILL to process {pid}."),
        )),
        "suspend" | "stop" | "sigstop" => Ok(signal_process_action_plan(
            "suspend",
            "STOP",
            vec![pid],
            "Dry run only; no suspend signal was sent.",
            format!("Sent STOP to process {pid}."),
        )),
        "resume" | "continue" | "cont" | "sigcont" => Ok(signal_process_action_plan(
            "resume",
            "CONT",
            vec![pid],
            "Dry run only; no resume signal was sent.",
            format!("Sent CONT to process {pid}."),
        )),
        "terminate-tree" | "tree-terminate" | "sigterm-tree" => Ok(signal_process_action_plan(
            "terminate-tree",
            "TERM",
            process_tree_target_pids(snapshot, pid),
            "Dry run only; no terminate-tree signal was sent.",
            format!("Sent TERM to process tree rooted at {pid}."),
        )),
        "force-kill-tree" | "tree-kill" | "tree-force-kill" | "sigkill-tree" => {
            Ok(signal_process_action_plan(
                "force-kill-tree",
                "KILL",
                process_tree_target_pids(snapshot, pid),
                "Dry run only; no force-kill-tree signal was sent.",
                format!("Sent KILL to process tree rooted at {pid}."),
            ))
        }
        "lower-priority" | "renice-background" | "background" => Ok(renice_process_action_plan(
            "lower-priority",
            10,
            pid,
            "Dry run only; priority was not changed.",
            format!("Requested lower priority for process {pid} with nice value 10."),
        )),
        "normal-priority" | "restore-priority" | "renice-normal" => Ok(renice_process_action_plan(
            "normal-priority",
            0,
            pid,
            "Dry run only; priority was not changed.",
            format!("Requested normal priority for process {pid} with nice value 0."),
        )),
        _ => Err(format!(
            "Unsupported process action '{action}'. Use terminate, force-kill, suspend, resume, terminate-tree, force-kill-tree, lower-priority, or normal-priority."
        )),
    }
}

pub(crate) fn signal_process_action_plan(
    normalized_action: &str,
    signal: &str,
    target_pids: Vec<u32>,
    dry_run_message: &str,
    success_message: String,
) -> ProcessActionPlan {
    let mut args = vec![format!("-{signal}")];
    args.extend(target_pids.iter().map(u32::to_string));
    ProcessActionPlan {
        normalized_action: normalized_action.to_owned(),
        signal: signal.to_owned(),
        command: format!("/bin/kill {}", args.join(" ")),
        program: "/bin/kill".to_owned(),
        args,
        target_pids,
        dry_run_message: dry_run_message.to_owned(),
        success_message,
    }
}

pub(crate) fn renice_process_action_plan(
    normalized_action: &str,
    nice_value: i32,
    pid: u32,
    dry_run_message: &str,
    success_message: String,
) -> ProcessActionPlan {
    let args = vec![nice_value.to_string(), "-p".to_owned(), pid.to_string()];
    ProcessActionPlan {
        normalized_action: normalized_action.to_owned(),
        signal: format!("renice:{nice_value}"),
        command: format!("/usr/bin/renice {}", args.join(" ")),
        program: "/usr/bin/renice".to_owned(),
        args,
        target_pids: vec![pid],
        dry_run_message: dry_run_message.to_owned(),
        success_message,
    }
}

pub(crate) fn process_tree_target_pids(snapshot: Option<&SystemSnapshot>, pid: u32) -> Vec<u32> {
    let mut target_pids = snapshot
        .map(|snapshot| process_descendant_pids(snapshot, pid))
        .unwrap_or_default();
    target_pids.push(pid);
    target_pids.sort_unstable();
    target_pids.dedup();
    target_pids.retain(|target_pid| *target_pid != 0);
    target_pids
}

pub(crate) fn process_descendant_pids(snapshot: &SystemSnapshot, pid: u32) -> Vec<u32> {
    let mut descendants = Vec::new();
    let mut stack = vec![pid];
    while let Some(parent_pid) = stack.pop() {
        for child_pid in process_child_pids(snapshot, parent_pid) {
            if descendants.contains(&child_pid) {
                continue;
            }
            descendants.push(child_pid);
            stack.push(child_pid);
        }
    }
    descendants
}

pub(crate) fn process_action_diagnostics_event(report: &ProcessActionReport) -> DiagnosticsEvent {
    let mut builder = DiagnosticsEvent::builder(
        if report.success {
            DiagnosticsLevel::Info
        } else {
            DiagnosticsLevel::Warn
        },
        DiagnosticsSubsystem::Engine,
        "process-action",
        report.message.clone(),
    )
    .field("pid", report.pid)
    .field(
        "target_pids",
        report
            .target_pids
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(","),
    )
    .field("target_count", report.target_pids.len())
    .field("action", report.action.clone())
    .field("signal", report.signal.clone())
    .field("success", report.success)
    .field("command", report.command.clone());
    if let Some(reason) = report.reason.as_ref() {
        builder = builder.field("reason", reason);
    }
    if let Some(entity_id) = report.entity_id.as_ref() {
        builder = builder.entity_id(entity_id.clone());
    }
    if let Some(display_name) = report.display_name.as_ref() {
        builder = builder.field("display_name", display_name);
    }
    builder.build()
}

pub(crate) fn process_action_history_item(event: DiagnosticsEvent) -> ProcessActionHistoryItem {
    let display_name = diagnostics_field(&event, "display_name").map(str::to_owned);
    ProcessActionHistoryItem {
        timestamp_millis: event.timestamp_millis,
        pid: diagnostics_field(&event, "pid").and_then(|value| value.parse::<u32>().ok()),
        target_pids: diagnostics_field(&event, "target_pids")
            .map(parse_pid_list)
            .unwrap_or_default(),
        action: diagnostics_field(&event, "action").map(str::to_owned),
        signal: diagnostics_field(&event, "signal").map(str::to_owned),
        success: diagnostics_field(&event, "success")
            .and_then(|value| value.parse::<bool>().ok())
            .unwrap_or(
                event.level != DiagnosticsLevel::Warn && event.level != DiagnosticsLevel::Error,
            ),
        reason: diagnostics_field(&event, "reason").map(str::to_owned),
        entity_id: event.entity_id,
        display_name,
        message: event.message,
    }
}

pub(crate) fn parse_pid_list(value: &str) -> Vec<u32> {
    value
        .split(',')
        .filter_map(|part| part.trim().parse::<u32>().ok())
        .collect()
}

pub(crate) fn diagnostics_field<'a>(event: &'a DiagnosticsEvent, key: &str) -> Option<&'a str> {
    event
        .fields
        .iter()
        .find(|field| field.key == key)
        .map(|field| field.value.as_str())
}

pub(crate) fn run_os_command(program: &str, args: &[String]) -> Result<String, String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|error| format!("run {}: {error}", program))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(if stderr.is_empty() {
            format!("{} exited with status {}", program, output.status)
        } else {
            format!("{} failed: {}", program, stderr)
        });
    }
    String::from_utf8(output.stdout).map_err(|error| format!("decode {} output: {error}", program))
}

pub(crate) fn parse_vmmap_regions(output: &str) -> Vec<MemoryRegionBreakdown> {
    output.lines().filter_map(parse_vmmap_region_line).collect()
}

pub(crate) fn parse_vmmap_region_line(line: &str) -> Option<MemoryRegionBreakdown> {
    let trimmed = line.trim_end();
    let bracket_start = trimmed.find('[')?;
    let bracket_end = trimmed[bracket_start..].find(']')? + bracket_start;
    let prefix = &trimmed[..bracket_start];
    let prefix_tokens = prefix.split_whitespace().collect::<Vec<_>>();
    let address_index = prefix_tokens.iter().position(|token| {
        token.contains('-')
            && token.split('-').all(|part| {
                !part.is_empty() && part.chars().all(|character| character.is_ascii_hexdigit())
            })
    })?;
    let region_type = prefix_tokens[..address_index].join(" ");
    if region_type.is_empty() || region_type == "REGION TYPE" {
        return None;
    }
    let stats = trimmed[bracket_start + 1..bracket_end]
        .split_whitespace()
        .collect::<Vec<_>>();
    if stats.len() < 4 {
        return None;
    }
    Some(MemoryRegionBreakdown {
        region_type,
        virtual_bytes: parse_vmmap_bytes(stats[0]),
        resident_bytes: parse_vmmap_bytes(stats[1]),
        dirty_bytes: parse_vmmap_bytes(stats[2]),
        swap_bytes: parse_vmmap_bytes(stats[3]),
    })
}

pub(crate) fn parse_vmmap_bytes(value: &str) -> u64 {
    let numeric = value.trim_end_matches(|character: char| character.is_ascii_alphabetic());
    let suffix = value[numeric.len()..].to_ascii_uppercase();
    let number = numeric.parse::<f64>().unwrap_or(0.0);
    let multiplier = match suffix.as_str() {
        "K" => 1024.0,
        "M" => 1024.0 * 1024.0,
        "G" => 1024.0 * 1024.0 * 1024.0,
        "T" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => 1.0,
    };
    (number * multiplier).round() as u64
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
    if joined.contains("cvdisplaylink") || joined.contains("displaylink") {
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

pub(crate) fn extract_tool_error_message(value: &Value) -> String {
    value
        .get("content")
        .and_then(Value::as_array)
        .and_then(|content| content.first())
        .and_then(|item| item.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("tool request failed")
        .to_owned()
}

pub(crate) fn format_metric_value(metric: &str, value: f64) -> String {
    match metric {
        "memory_bytes" => format_bytes(value.max(0.0).round() as u64),
        "disk_activity_bps" | "network_activity_bps" => {
            format!("{}/s", format_bytes(value.max(0.0).round() as u64))
        }
        _ => format!("{value:.1}"),
    }
}

#[cfg(test)]
mod metadata_tests {
    use super::*;

    #[test]
    fn parse_procargs2_extracts_env_after_argv() {
        let mut buffer = Vec::new();
        buffer.extend_from_slice(&1i32.to_ne_bytes()); // argc = 1
        buffer.extend_from_slice(b"/bin/exec\0"); // executable path
        buffer.extend_from_slice(b"/bin/exec\0"); // argv[0]
        buffer.extend_from_slice(b"FOO=bar\0");
        buffer.extend_from_slice(b"SECRET_KEY=zzz\0");
        buffer.extend_from_slice(b"\0"); // empty string terminates env

        let envs = parse_procargs2(&buffer);
        assert_eq!(
            envs,
            vec![
                ("FOO".to_owned(), "bar".to_owned()),
                ("SECRET_KEY".to_owned(), "zzz".to_owned()),
            ]
        );
    }

    #[test]
    fn parse_procargs2_handles_truncated_buffer() {
        assert!(parse_procargs2(&[]).is_empty());
        assert!(parse_procargs2(&[0, 1]).is_empty());
    }

    #[test]
    fn value_is_secret_flags_keys_and_known_prefixes() {
        assert!(value_is_secret("AWS_SECRET_ACCESS_KEY", "x"));
        assert!(value_is_secret("GITHUB_TOKEN", "x"));
        assert!(value_is_secret("anything", "sk-abc123"));
        assert!(value_is_secret("anything", "ghp_abc"));
        assert!(!value_is_secret("PATH", "/usr/bin:/bin"));
        assert!(!value_is_secret("HOME", "/Users/someone"));
    }

    #[test]
    fn classify_signature_distinguishes_authorities() {
        let dev_id = vec![
            "Developer ID Application: Acme Inc (TEAMID)".to_owned(),
            "Developer ID Certification Authority".to_owned(),
            "Apple Root CA".to_owned(),
        ];
        assert_eq!(
            classify_signature(true, &dev_id),
            ("developer_id".to_owned(), false)
        );

        let mas = vec![
            "Apple Mac OS Application Signing".to_owned(),
            "Apple Worldwide Developer Relations Certification Authority".to_owned(),
            "Apple Root CA".to_owned(),
        ];
        assert_eq!(
            classify_signature(true, &mas),
            ("mac_app_store".to_owned(), false)
        );

        let apple = vec![
            "Software Signing".to_owned(),
            "Apple Code Signing Certification Authority".to_owned(),
            "Apple Root CA".to_owned(),
        ];
        assert_eq!(
            classify_signature(true, &apple),
            ("apple".to_owned(), false)
        );

        assert_eq!(classify_signature(true, &[]), ("adhoc".to_owned(), true));
        assert_eq!(
            classify_signature(false, &[]),
            ("unsigned".to_owned(), false)
        );
    }

    #[test]
    fn parse_dylibs_classifies_and_flags_injected() {
        let output = "COMMAND PID USER FD TYPE DEVICE SIZE/OFF NODE NAME\n\
            App 100 alice txt REG 1,4 100 200 /usr/lib/libSystem.B.dylib\n\
            App 100 alice txt REG 1,4 100 200 /opt/acme/libplugin.dylib\n\
            App 100 alice txt REG 1,4 100 200 /tmp/inject.dylib\n\
            App 100 alice 1u IPv4 0x1 0 0t0 TCP 1.2.3.4:443\n";
        let injected = ["/tmp/inject.dylib"];
        let (dylibs, summary) = parse_dylibs_from_lsof(output, &injected);
        assert_eq!(summary.total, 3);
        assert_eq!(summary.third_party, 2);
        assert_eq!(summary.injected, 1);
        assert!(
            dylibs.iter().any(|d| d.path == "/tmp/inject.dylib"
                && d.injected
                && d.category == "third_party")
        );
        assert!(
            dylibs
                .iter()
                .any(|d| d.path == "/usr/lib/libSystem.B.dylib" && d.category == "system")
        );
    }
}
