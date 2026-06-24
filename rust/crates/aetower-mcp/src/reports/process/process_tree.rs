use std::collections::BTreeMap;

use aetower_model::SystemSnapshot;

use super::*;

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
    let mut aggregate_cache = BTreeMap::new();
    let total_aggregate = roots
        .iter()
        .fold(ProcessAggregate::default(), |aggregate, root| {
            let next = subtree_aggregate_cached(root, &children, &mut aggregate_cache);
            ProcessAggregate {
                subtree_cpu_percent: aggregate.subtree_cpu_percent + next.subtree_cpu_percent,
                subtree_memory_bytes: aggregate.subtree_memory_bytes + next.subtree_memory_bytes,
                subtree_process_count: aggregate.subtree_process_count + next.subtree_process_count,
            }
        });

    let mut sorted_roots = roots
        .into_iter()
        .map(|root| {
            let aggregate = subtree_aggregate_cached(&root, &children, &mut aggregate_cache);
            (root, aggregate)
        })
        .collect::<Vec<_>>();
    sorted_roots.sort_by(|left, right| compare_root_aggregate(&left.1, &right.1));

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
                children: {
                    let mut child_nodes = Vec::new();
                    for (root, _) in &sorted_roots {
                        child_nodes.push(process_tree_node(
                            root,
                            &children,
                            "session-child",
                            &mut aggregate_cache,
                        ));
                    }
                    child_nodes
                },
            });
        }
    } else {
        for (root, _) in &sorted_roots {
            reports.push(process_tree_node(
                root,
                &children,
                "process-root",
                &mut aggregate_cache,
            ));
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
    aggregate_cache: &mut BTreeMap<u32, ProcessAggregate>,
) -> ProcessTreeNodeReport {
    let aggregate = subtree_aggregate_cached(related, children, aggregate_cache);
    let child_nodes = related
        .component
        .process_id
        .and_then(|pid| children.get(&pid))
        .map(|child_components| {
            let mut child_components = child_components
                .iter()
                .cloned()
                .map(|child| {
                    let aggregate = subtree_aggregate_cached(&child, children, aggregate_cache);
                    (child, aggregate)
                })
                .collect::<Vec<_>>();
            child_components.sort_by(|left, right| compare_child_aggregate(&left.1, &right.1));
            child_components
                .iter()
                .map(|(child, _)| process_tree_node(child, children, "child", aggregate_cache))
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

fn subtree_aggregate_cached(
    related: &RelatedProcessComponent,
    children: &BTreeMap<u32, Vec<RelatedProcessComponent>>,
    cache: &mut BTreeMap<u32, ProcessAggregate>,
) -> ProcessAggregate {
    if let Some(pid) = related.component.process_id
        && let Some(aggregate) = cache.get(&pid)
    {
        return *aggregate;
    }
    let mut aggregate = ProcessAggregate {
        subtree_cpu_percent: related.component.cpu_percent,
        subtree_memory_bytes: related.component.memory_bytes,
        subtree_process_count: 1,
    };
    if let Some(pid) = related.component.process_id
        && let Some(child_components) = children.get(&pid)
    {
        for child in child_components {
            let child_aggregate = subtree_aggregate_cached(child, children, cache);
            aggregate.subtree_cpu_percent += child_aggregate.subtree_cpu_percent;
            aggregate.subtree_memory_bytes += child_aggregate.subtree_memory_bytes;
            aggregate.subtree_process_count += child_aggregate.subtree_process_count;
        }
    }
    if let Some(pid) = related.component.process_id {
        cache.insert(pid, aggregate);
    }
    aggregate
}

fn compare_root_aggregate(left: &ProcessAggregate, right: &ProcessAggregate) -> std::cmp::Ordering {
    right
        .subtree_cpu_percent
        .partial_cmp(&left.subtree_cpu_percent)
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| right.subtree_memory_bytes.cmp(&left.subtree_memory_bytes))
}

fn compare_child_aggregate(
    left: &ProcessAggregate,
    right: &ProcessAggregate,
) -> std::cmp::Ordering {
    right
        .subtree_cpu_percent
        .partial_cmp(&left.subtree_cpu_percent)
        .unwrap_or(std::cmp::Ordering::Equal)
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
