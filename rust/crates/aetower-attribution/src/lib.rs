use std::collections::{BTreeMap, BTreeSet};

use aetower_collector::{RawProcessSample, index_processes};
use aetower_identity::{EntitySeed, IdentityMap};
use aetower_model::{
    AggregateMetrics, AttributionConfidence, ComponentKind, ComponentSnapshot, EntityKind,
    EntitySnapshot, FrontmostAppState, ProcessLineageNode, ProvenanceKind, ProvenanceSnapshot,
};

const MAX_COMPONENT_DETAIL_CHARS: usize = 160;

pub fn build_entities(
    processes: &[RawProcessSample],
    identity: &IdentityMap,
    frontmost: Option<&FrontmostAppState>,
) -> Vec<EntitySnapshot> {
    let process_index = index_processes(processes);
    let mut grouped: BTreeMap<String, EntitySnapshot> = BTreeMap::new();

    for process in processes {
        let Some(entity_id) = identity.pid_to_entity.get(&process.pid) else {
            continue;
        };
        let Some(seed) = identity.entities.get(entity_id) else {
            continue;
        };

        let entry = grouped
            .entry(entity_id.clone())
            .or_insert_with(|| entity_from_seed(seed));

        if process.start_time_millis > 0 {
            entry.oldest_process_start_millis = match entry.oldest_process_start_millis {
                0 => process.start_time_millis,
                current => current.min(process.start_time_millis),
            };
            entry.newest_process_start_millis = entry
                .newest_process_start_millis
                .max(process.start_time_millis);
        }

        entry.metrics.cpu_percent += process.cpu_percent;
        entry.metrics.memory_resident_bytes = entry
            .metrics
            .memory_resident_bytes
            .saturating_add(process.memory_bytes);
        entry.metrics.memory_physical_footprint_bytes = entry
            .metrics
            .memory_physical_footprint_bytes
            .saturating_add(process.memory_physical_footprint_bytes);
        // Per-process disk fields are true per-second rates now, so summing
        // them yields a correct aggregate rate (previously this summed raw
        // per-tick byte deltas into a *_bps field — a byte count mislabelled
        // as a rate).
        entry.metrics.disk_read_bps = entry
            .metrics
            .disk_read_bps
            .saturating_add(process.disk_read_bps);
        entry.metrics.disk_write_bps = entry
            .metrics
            .disk_write_bps
            .saturating_add(process.disk_write_bps);
        entry.metrics.wakeups_per_second += process.wakeups_per_second;
        entry.metrics.energy_nj_per_s += process.energy_nj_per_s;
        entry.metrics.process_count += 1;
        entry.metrics.thread_count = entry
            .metrics
            .thread_count
            .saturating_add(process.thread_count);
        entry.metrics.is_foreground =
            entry.metrics.is_foreground || is_foreground_match(seed, frontmost);
        if entry.metrics.is_foreground {
            entry.active_window_title = frontmost.and_then(|state| state.window_title.clone());
        }

        let ancestor_roles =
            collect_ancestor_roles(process, &seed.entity_id, &process_index, identity);

        entry.components.push(ComponentSnapshot {
            kind: if seed.entity_kind == EntityKind::TerminalSession {
                ComponentKind::Command
            } else {
                ComponentKind::Process
            },
            title: process.identity.name.clone(),
            detail: summarize_process(process),
            adapter_context: None,
            provenance: component_provenance(process, &ancestor_roles),
            process_id: Some(process.pid),
            start_time_millis: process.start_time_millis,
            executable_path: process.identity.exe.clone(),
            command_line: command_line(process),
            parent_summary: parent_summary(process, &process_index),
            launched_by: launched_by(&ancestor_roles),
            cpu_percent: process.cpu_percent,
            memory_bytes: process.memory_bytes,
            memory_physical_footprint_bytes: process.memory_physical_footprint_bytes,
            cwd: process.cwd.clone(),
            user: process.identity.user.clone(),
            thread_count: process.thread_count,
        });

        entry.process_lineage.push(ProcessLineageNode {
            pid: process.pid,
            parent_pid: process.parent_pid,
            entity_id: seed.entity_id.clone(),
            title: process.identity.name.clone(),
            start_time_millis: process.start_time_millis,
            executable_path: process.identity.exe.clone(),
            command_line: command_line(process),
            cwd: process.cwd.clone(),
            user: process.identity.user.clone(),
            session_id: None,
            workspace: process.cwd.clone(),
            cpu_percent: process.cpu_percent,
            memory_bytes: process.memory_bytes,
            memory_physical_footprint_bytes: process.memory_physical_footprint_bytes,
            thread_count: process.thread_count,
            source: "collector-parent-pid".to_owned(),
            confidence: 0.95,
        });
    }

    let mut entities: Vec<_> = grouped.into_values().collect();
    for entity in &mut entities {
        entity.components.sort_by(|left, right| {
            right
                .cpu_percent
                .partial_cmp(&left.cpu_percent)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| right.memory_bytes.cmp(&left.memory_bytes))
                .then_with(|| left.title.cmp(&right.title))
        });
        entity.components.truncate(8);
        summarize_entity_attribution(entity);
    }
    entities
}

fn entity_from_seed(seed: &EntitySeed) -> EntitySnapshot {
    EntitySnapshot {
        entity_id: seed.entity_id.clone(),
        display_name: seed.display_name.clone(),
        primary_provenance: entity_provenance(seed),
        launcher_summary: None,
        attribution_notes: Vec::new(),
        bundle_id: seed.bundle_id.clone(),
        executable_path: seed.executable_path.clone(),
        oldest_process_start_millis: 0,
        newest_process_start_millis: 0,
        entity_kind: seed.entity_kind.clone(),
        metrics: AggregateMetrics::default(),
        friction: Default::default(),
        components: Vec::new(),
        process_lineage: Vec::new(),
        trend: Default::default(),
        badges: seed.badges.clone(),
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
        app_version: None,
    }
}

fn summarize_entity_attribution(entity: &mut EntitySnapshot) {
    let launchers: BTreeSet<String> = entity
        .components
        .iter()
        .filter_map(|component| component.launched_by.clone())
        .filter(|value| !value.is_empty())
        .collect();
    entity.launcher_summary = match launchers.len() {
        0 => None,
        1 => launchers.first().cloned(),
        _ => {
            let summary = launchers
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            Some(format!("mixed lineage via {summary}"))
        }
    };

    let low_confidence_components = entity
        .components
        .iter()
        .filter(|component| {
            component
                .provenance
                .as_ref()
                .map(|provenance| provenance.confidence == AttributionConfidence::Low)
                .unwrap_or(false)
        })
        .count();
    if low_confidence_components > 0 {
        entity.attribution_notes.push(format!(
            "{} component(s) rely on low-confidence parent fallback attribution.",
            low_confidence_components
        ));
    }

    if launchers.len() > 1 {
        entity.attribution_notes.push(format!(
            "Components were launched by more than one parent lineage: {}.",
            launchers
                .iter()
                .take(4)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
        if let Some(primary) = entity.primary_provenance.as_mut()
            && primary.confidence == AttributionConfidence::High
        {
            primary.confidence = AttributionConfidence::Medium;
            if primary.rule.is_empty() {
                primary.rule = "mixed component launch lineage".to_owned();
            } else {
                primary.rule = format!("{}; mixed component launch lineage", primary.rule);
            }
        }
    }

    let missing_exec = entity
        .components
        .iter()
        .filter(|component| {
            matches!(
                component.kind,
                ComponentKind::Process | ComponentKind::Command
            ) && component.executable_path.is_none()
        })
        .count();
    if missing_exec > 0 {
        entity.attribution_notes.push(format!(
            "{} process component(s) are missing an executable path, so attribution is based on process-tree heuristics.",
            missing_exec
        ));
    }
}

fn entity_provenance(seed: &EntitySeed) -> Option<ProvenanceSnapshot> {
    if seed.badges.iter().any(|badge| badge == "user-launch") {
        return Some(provenance(
            ProvenanceKind::UserLaunch,
            "User-launched app",
            "bundle-root + user-launch lineage",
            AttributionConfidence::High,
        ));
    }
    if seed.badges.iter().any(|badge| badge == "xpc-service") {
        return Some(provenance(
            ProvenanceKind::XpcService,
            "XPC service",
            "xpc-service executable path",
            AttributionConfidence::High,
        ));
    }
    if seed.entity_kind == EntityKind::TerminalSession {
        return Some(provenance(
            ProvenanceKind::ShellSession,
            "Interactive shell session",
            "shell lineage",
            AttributionConfidence::High,
        ));
    }
    if seed.badges.iter().any(|badge| badge == "login-item") {
        return Some(provenance(
            ProvenanceKind::LoginItem,
            "Login item",
            "loginwindow/xpcproxy lineage",
            AttributionConfidence::Medium,
        ));
    }
    if seed.badges.iter().any(|badge| badge == "launchd-managed") {
        return Some(provenance(
            ProvenanceKind::ServiceManager,
            "launchd-managed service",
            "launchd lineage",
            AttributionConfidence::High,
        ));
    }
    if seed.badges.iter().any(|badge| badge == "helper-group") {
        return Some(provenance(
            ProvenanceKind::HelperTree,
            "Grouped app helper tree",
            "bundle helper lineage",
            AttributionConfidence::Medium,
        ));
    }
    if seed.bundle_id.is_some() || seed.entity_kind == EntityKind::App {
        return Some(provenance(
            ProvenanceKind::AppBundle,
            "Application bundle",
            "bundle executable path",
            AttributionConfidence::High,
        ));
    }
    None
}

fn summarize_process(process: &RawProcessSample) -> String {
    if process.cmd().is_empty() {
        format!("pid {}", process.pid)
    } else {
        compact_component_detail(&process.cmd().join(" "))
    }
}

fn command_line(process: &RawProcessSample) -> Option<String> {
    (!process.cmd().is_empty()).then(|| process.cmd().join(" "))
}

fn compact_component_detail(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= MAX_COMPONENT_DETAIL_CHARS {
        return trimmed.to_owned();
    }
    let head: String = trimmed
        .chars()
        .take(MAX_COMPONENT_DETAIL_CHARS.saturating_sub(3))
        .collect();
    format!("{head}...")
}

fn parent_summary(
    process: &RawProcessSample,
    process_index: &BTreeMap<u32, &RawProcessSample>,
) -> Option<String> {
    let parent_pid = process.parent_pid?;
    let parent = process_index.get(&parent_pid)?;
    Some(format_process_label(parent))
}

#[derive(Default)]
struct AncestorRoles<'a> {
    immediate_parent: Option<&'a RawProcessSample>,
    top_same_entity_ancestor: Option<&'a RawProcessSample>,
    shell_ancestor: Option<&'a RawProcessSample>,
    login_item_ancestor: Option<&'a RawProcessSample>,
    service_manager_ancestor: Option<&'a RawProcessSample>,
}

fn collect_ancestor_roles<'a>(
    process: &RawProcessSample,
    entity_id: &str,
    process_index: &BTreeMap<u32, &'a RawProcessSample>,
    identity: &IdentityMap,
) -> AncestorRoles<'a> {
    let mut next_pid = process.parent_pid;
    let mut roles = AncestorRoles::default();

    while let Some(pid) = next_pid {
        let Some(ancestor) = process_index.get(&pid).copied() else {
            break;
        };
        if roles.immediate_parent.is_none() {
            roles.immediate_parent = Some(ancestor);
        }
        if identity
            .pid_to_entity
            .get(&ancestor.pid)
            .is_some_and(|ancestor_entity_id| ancestor_entity_id == entity_id)
        {
            roles.top_same_entity_ancestor = Some(ancestor);
        }
        if roles.shell_ancestor.is_none() && is_shell_process(ancestor) {
            roles.shell_ancestor = Some(ancestor);
        }
        if roles.login_item_ancestor.is_none() && is_login_item_launcher(ancestor) {
            roles.login_item_ancestor = Some(ancestor);
        }
        if roles.service_manager_ancestor.is_none() && is_service_manager(ancestor) {
            roles.service_manager_ancestor = Some(ancestor);
        }
        next_pid = ancestor.parent_pid;
    }

    roles
}

fn launched_by(ancestor_roles: &AncestorRoles<'_>) -> Option<String> {
    ancestor_roles
        .shell_ancestor
        .map(format_shell_session_label)
        .or_else(|| {
            ancestor_roles
                .top_same_entity_ancestor
                .map(format_process_label)
        })
        .or_else(|| {
            ancestor_roles
                .login_item_ancestor
                .map(format_login_item_label)
        })
        .or_else(|| {
            ancestor_roles
                .service_manager_ancestor
                .map(format_service_manager_label)
        })
        .or_else(|| ancestor_roles.immediate_parent.map(format_process_label))
}

fn component_provenance(
    process: &RawProcessSample,
    ancestor_roles: &AncestorRoles<'_>,
) -> Option<ProvenanceSnapshot> {
    if process
        .exe()
        .is_some_and(|path| path.contains(".xpc/") || path.contains("/Contents/XPCServices/"))
    {
        return Some(provenance(
            ProvenanceKind::XpcService,
            "XPC service process",
            "xpc-service executable path",
            AttributionConfidence::High,
        ));
    }

    if let Some(shell) = ancestor_roles.shell_ancestor {
        Some(provenance(
            ProvenanceKind::ShellSession,
            &format!("Shell session via {}", process_display_name(shell)),
            "shell lineage",
            AttributionConfidence::High,
        ))
    } else if ancestor_roles.top_same_entity_ancestor.is_some() {
        Some(provenance(
            ProvenanceKind::HelperTree,
            "App helper subprocess",
            "same-entity ancestor lineage",
            AttributionConfidence::Medium,
        ))
    } else if let Some(login_item) = ancestor_roles.login_item_ancestor {
        Some(provenance(
            ProvenanceKind::LoginItem,
            &format!("Login item via {}", process_display_name(login_item)),
            "loginwindow/xpcproxy lineage",
            AttributionConfidence::Medium,
        ))
    } else if ancestor_roles.service_manager_ancestor.is_some() {
        Some(provenance(
            ProvenanceKind::ServiceManager,
            "launchd service manager",
            "launchd lineage",
            AttributionConfidence::High,
        ))
    } else {
        ancestor_roles.immediate_parent.map(|parent| {
            provenance(
                ProvenanceKind::ParentProcess,
                &format!("Parent process {}", process_display_name(parent)),
                "immediate parent fallback",
                AttributionConfidence::Low,
            )
        })
    }
}

fn format_process_label(process: &RawProcessSample) -> String {
    format!("{} (pid {})", process_display_name(process), process.pid)
}

fn process_display_name(process: &RawProcessSample) -> String {
    if !process.name().is_empty() {
        process.identity.name.clone()
    } else if let Some(path) = process.exe() {
        std::path::Path::new(path)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(path)
            .to_owned()
    } else {
        "Unknown process".to_owned()
    }
}

fn is_shell_process(process: &RawProcessSample) -> bool {
    matches!(process.name(), "zsh" | "bash" | "fish" | "sh")
}

fn is_login_item_launcher(process: &RawProcessSample) -> bool {
    matches!(process.name(), "loginwindow" | "xpcproxy")
}

fn is_service_manager(process: &RawProcessSample) -> bool {
    process.name() == "launchd"
}

fn format_shell_session_label(process: &RawProcessSample) -> String {
    format!(
        "{} shell session (pid {})",
        process_display_name(process),
        process.pid
    )
}

fn format_login_item_label(process: &RawProcessSample) -> String {
    match process.name() {
        "xpcproxy" => format!("xpcproxy login item launcher (pid {})", process.pid),
        "loginwindow" => format!("loginwindow session launcher (pid {})", process.pid),
        _ => format_process_label(process),
    }
}

fn format_service_manager_label(process: &RawProcessSample) -> String {
    format!("launchd service manager (pid {})", process.pid)
}

fn provenance(
    kind: ProvenanceKind,
    label: &str,
    rule: &str,
    confidence: AttributionConfidence,
) -> ProvenanceSnapshot {
    ProvenanceSnapshot {
        kind,
        label: label.to_owned(),
        rule: rule.to_owned(),
        confidence,
    }
}

#[allow(clippy::collapsible_if)]
fn is_foreground_match(seed: &EntitySeed, frontmost: Option<&FrontmostAppState>) -> bool {
    let Some(frontmost) = frontmost else {
        return false;
    };

    if let (Some(seed_bundle), Some(frontmost_bundle)) =
        (seed.bundle_id.as_deref(), frontmost.bundle_id.as_deref())
        && seed_bundle == frontmost_bundle
    {
        return true;
    }

    if let (Some(seed_path), Some(frontmost_path)) = (
        seed.executable_path.as_deref(),
        frontmost.executable_path.as_deref(),
    ) && seed_path == frontmost_path
    {
        return true;
    }

    seed.display_name == frontmost.app_name
}

#[cfg(test)]
mod tests {
    use aetower_model::{EntityKind, FrontmostAppState};

    use super::*;

    #[test]
    fn grouped_entities_sum_memory_across_processes() {
        let processes = vec![
            RawProcessSample {
                start_time_millis: 10,
                cpu_percent: 10.0,
                memory_bytes: 128,
                disk_read_bps: 100,
                disk_write_bps: 200,
                ..RawProcessSample::synthetic(
                    1,
                    None,
                    "Renderer",
                    Some("/Applications/Test.app/Contents/MacOS/Test"),
                    &[],
                )
            },
            RawProcessSample {
                start_time_millis: 20,
                cpu_percent: 5.0,
                memory_bytes: 256,
                disk_read_bps: 300,
                disk_write_bps: 400,
                ..RawProcessSample::synthetic(
                    2,
                    Some(1),
                    "Helper",
                    Some("/Applications/Test.app/Contents/MacOS/Test Helper"),
                    &[],
                )
            },
        ];
        let entity_id = "bundle:test".to_owned();
        let identity = IdentityMap {
            entities: std::iter::once((
                entity_id.clone(),
                EntitySeed {
                    entity_id: entity_id.clone(),
                    display_name: "Test".to_owned(),
                    bundle_id: Some("local.test".to_owned()),
                    executable_path: Some("/Applications/Test.app/Contents/MacOS/Test".to_owned()),
                    entity_kind: EntityKind::App,
                    badges: Vec::new(),
                },
            ))
            .collect(),
            pid_to_entity: [(1, entity_id.clone()), (2, entity_id)]
                .into_iter()
                .collect(),
        };

        let entities = build_entities(&processes, &identity, None);
        let entity = &entities[0];

        assert_eq!(entity.metrics.memory_resident_bytes, 384);
        assert_eq!(entity.metrics.disk_read_bps, 400);
        assert_eq!(entity.metrics.disk_write_bps, 600);
        assert_eq!(
            entity
                .primary_provenance
                .as_ref()
                .map(|value| value.rule.as_str()),
            Some("bundle executable path")
        );
    }

    #[test]
    fn entities_are_not_foreground_without_frontmost_state() {
        let seed = EntitySeed {
            entity_id: "bundle:test".to_owned(),
            display_name: "Test".to_owned(),
            bundle_id: Some("local.test".to_owned()),
            executable_path: Some("/Applications/Test.app/Contents/MacOS/Test".to_owned()),
            entity_kind: EntityKind::App,
            badges: Vec::new(),
        };

        assert!(!is_foreground_match(&seed, None));
        assert!(is_foreground_match(
            &seed,
            Some(&FrontmostAppState {
                app_name: "Test".to_owned(),
                bundle_id: Some("local.test".to_owned()),
                executable_path: None,
                window_title: None,
                captured_at_millis: 0,
            })
        ));
    }

    #[test]
    fn process_component_tracks_parent_and_launcher() {
        let processes = vec![
            RawProcessSample {
                start_time_millis: 10,
                cpu_percent: 1.0,
                memory_bytes: 128,
                ..RawProcessSample::synthetic(
                    10,
                    Some(1),
                    "Test",
                    Some("/Applications/Test.app/Contents/MacOS/Test"),
                    &["/Applications/Test.app/Contents/MacOS/Test"],
                )
            },
            RawProcessSample {
                start_time_millis: 20,
                cpu_percent: 5.0,
                memory_bytes: 256,
                ..RawProcessSample::synthetic(
                    11,
                    Some(10),
                    "Test Helper",
                    Some("/Applications/Test.app/Contents/MacOS/Test Helper"),
                    &[
                        "/Applications/Test.app/Contents/MacOS/Test Helper",
                        "--type=renderer",
                    ],
                )
            },
            RawProcessSample {
                start_time_millis: 1,
                ..RawProcessSample::synthetic(
                    1,
                    None,
                    "launchd",
                    Some("/sbin/launchd"),
                    &["/sbin/launchd"],
                )
            },
        ];
        let identity = IdentityMap {
            entities: [
                (
                    "bundle:test".to_owned(),
                    EntitySeed {
                        entity_id: "bundle:test".to_owned(),
                        display_name: "Test".to_owned(),
                        bundle_id: Some("local.test".to_owned()),
                        executable_path: Some(
                            "/Applications/Test.app/Contents/MacOS/Test".to_owned(),
                        ),
                        entity_kind: EntityKind::App,
                        badges: Vec::new(),
                    },
                ),
                (
                    "process:launchd".to_owned(),
                    EntitySeed {
                        entity_id: "process:launchd".to_owned(),
                        display_name: "launchd".to_owned(),
                        bundle_id: None,
                        executable_path: Some("/sbin/launchd".to_owned()),
                        entity_kind: EntityKind::Service,
                        badges: Vec::new(),
                    },
                ),
            ]
            .into_iter()
            .collect(),
            pid_to_entity: [
                (10, "bundle:test".to_owned()),
                (11, "bundle:test".to_owned()),
                (1, "process:launchd".to_owned()),
            ]
            .into_iter()
            .collect(),
        };

        let entities = build_entities(&processes, &identity, None);
        let helper_component = entities[0]
            .components
            .iter()
            .find(|component| component.process_id == Some(11))
            .expect("helper component should be present");

        assert_eq!(
            helper_component.parent_summary.as_deref(),
            Some("Test (pid 10)")
        );
        assert_eq!(
            helper_component.launched_by.as_deref(),
            Some("Test (pid 10)")
        );
        assert_eq!(
            helper_component.command_line.as_deref(),
            Some("/Applications/Test.app/Contents/MacOS/Test Helper --type=renderer")
        );
        assert_eq!(
            helper_component
                .provenance
                .as_ref()
                .map(|value| value.rule.as_str()),
            Some("same-entity ancestor lineage")
        );
    }

    #[test]
    fn process_detail_is_compacted_but_command_line_stays_complete() {
        let long_argument = "x".repeat(240);
        let process = RawProcessSample {
            start_time_millis: 1,
            ..RawProcessSample::synthetic(
                10,
                None,
                "python3",
                Some("/opt/homebrew/bin/python3"),
                &["python3", &long_argument],
            )
        };

        let detail = summarize_process(&process);
        let Some(command_line) = command_line(&process) else {
            panic!("command line should be present");
        };

        assert!(detail.len() <= MAX_COMPONENT_DETAIL_CHARS);
        assert!(detail.ends_with("..."));
        assert_eq!(command_line, format!("python3 {long_argument}"));
    }

    #[test]
    fn process_component_prefers_shell_session_for_external_launcher() {
        let processes = vec![
            RawProcessSample {
                start_time_millis: 10,
                ..RawProcessSample::synthetic(20, Some(10), "zsh", Some("/bin/zsh"), &["-zsh"])
            },
            RawProcessSample {
                start_time_millis: 20,
                cpu_percent: 1.0,
                memory_bytes: 64,
                ..RawProcessSample::synthetic(
                    21,
                    Some(20),
                    "python3",
                    Some("/opt/homebrew/bin/python3"),
                    &["python3", "server.py"],
                )
            },
            RawProcessSample {
                start_time_millis: 1,
                ..RawProcessSample::synthetic(
                    10,
                    None,
                    "Terminal",
                    Some("/System/Applications/Utilities/Terminal.app/Contents/MacOS/Terminal"),
                    &["Terminal"],
                )
            },
        ];
        let identity = IdentityMap {
            entities: [(
                "process:python3".to_owned(),
                EntitySeed {
                    entity_id: "process:python3".to_owned(),
                    display_name: "python3".to_owned(),
                    bundle_id: None,
                    executable_path: Some("/opt/homebrew/bin/python3".to_owned()),
                    entity_kind: EntityKind::Service,
                    badges: Vec::new(),
                },
            )]
            .into_iter()
            .collect(),
            pid_to_entity: [(21, "process:python3".to_owned())].into_iter().collect(),
        };

        let entities = build_entities(&processes, &identity, None);
        let component = &entities[0].components[0];
        assert_eq!(
            component.launched_by.as_deref(),
            Some("zsh shell session (pid 20)")
        );
    }

    #[test]
    fn process_component_labels_service_manager_launcher() {
        let processes = vec![
            RawProcessSample {
                start_time_millis: 20,
                cpu_percent: 1.0,
                memory_bytes: 64,
                ..RawProcessSample::synthetic(
                    31,
                    Some(1),
                    "sync-agent",
                    Some("/usr/libexec/sync-agent"),
                    &["/usr/libexec/sync-agent"],
                )
            },
            RawProcessSample {
                start_time_millis: 1,
                ..RawProcessSample::synthetic(
                    1,
                    None,
                    "launchd",
                    Some("/sbin/launchd"),
                    &["/sbin/launchd"],
                )
            },
        ];
        let identity = IdentityMap {
            entities: [(
                "process:sync-agent".to_owned(),
                EntitySeed {
                    entity_id: "process:sync-agent".to_owned(),
                    display_name: "sync-agent".to_owned(),
                    bundle_id: None,
                    executable_path: Some("/usr/libexec/sync-agent".to_owned()),
                    entity_kind: EntityKind::Service,
                    badges: Vec::new(),
                },
            )]
            .into_iter()
            .collect(),
            pid_to_entity: [(31, "process:sync-agent".to_owned())]
                .into_iter()
                .collect(),
        };

        let entities = build_entities(&processes, &identity, None);
        assert_eq!(
            entities[0].components[0].launched_by.as_deref(),
            Some("launchd service manager (pid 1)")
        );
    }

    #[test]
    fn process_component_labels_login_item_launcher() {
        let processes = vec![
            RawProcessSample {
                start_time_millis: 20,
                cpu_percent: 1.0,
                memory_bytes: 64,
                ..RawProcessSample::synthetic(
                    41,
                    Some(3),
                    "MenuBarExtra",
                    Some("/Users/test/Library/Application Support/Foo/MenuBarExtra"),
                    &["MenuBarExtra"],
                )
            },
            RawProcessSample {
                start_time_millis: 10,
                ..RawProcessSample::synthetic(
                    3,
                    Some(2),
                    "xpcproxy",
                    Some("/usr/libexec/xpcproxy"),
                    &["/usr/libexec/xpcproxy"],
                )
            },
            RawProcessSample {
                start_time_millis: 5,
                ..RawProcessSample::synthetic(
                    2,
                    Some(1),
                    "loginwindow",
                    Some("/System/Library/CoreServices/loginwindow.app/Contents/MacOS/loginwindow"),
                    &["loginwindow"],
                )
            },
            RawProcessSample {
                start_time_millis: 1,
                ..RawProcessSample::synthetic(
                    1,
                    None,
                    "launchd",
                    Some("/sbin/launchd"),
                    &["/sbin/launchd"],
                )
            },
        ];
        let identity = IdentityMap {
            entities: [(
                "process:menubarextra".to_owned(),
                EntitySeed {
                    entity_id: "process:menubarextra".to_owned(),
                    display_name: "MenuBarExtra".to_owned(),
                    bundle_id: None,
                    executable_path: Some(
                        "/Users/test/Library/Application Support/Foo/MenuBarExtra".to_owned(),
                    ),
                    entity_kind: EntityKind::Service,
                    badges: vec!["login-item".to_owned()],
                },
            )]
            .into_iter()
            .collect(),
            pid_to_entity: [(41, "process:menubarextra".to_owned())]
                .into_iter()
                .collect(),
        };

        let entities = build_entities(&processes, &identity, None);
        assert_eq!(
            entities[0].components[0].launched_by.as_deref(),
            Some("xpcproxy login item launcher (pid 3)")
        );
    }

    #[test]
    fn mixed_launchers_add_attribution_note_and_downgrade_confidence() {
        let processes = vec![
            RawProcessSample {
                start_time_millis: 10,
                cpu_percent: 1.0,
                memory_bytes: 64,
                ..RawProcessSample::synthetic(
                    10,
                    Some(1),
                    "Test Helper",
                    Some("/Applications/Test.app/Contents/MacOS/Test Helper"),
                    &["helper-a"],
                )
            },
            RawProcessSample {
                start_time_millis: 20,
                cpu_percent: 1.0,
                memory_bytes: 64,
                ..RawProcessSample::synthetic(
                    11,
                    Some(2),
                    "Test Helper",
                    Some("/Applications/Test.app/Contents/MacOS/Test Helper"),
                    &["helper-b"],
                )
            },
            RawProcessSample {
                start_time_millis: 1,
                ..RawProcessSample::synthetic(
                    1,
                    None,
                    "launchd",
                    Some("/sbin/launchd"),
                    &["/sbin/launchd"],
                )
            },
            RawProcessSample {
                start_time_millis: 2,
                ..RawProcessSample::synthetic(2, None, "zsh", Some("/bin/zsh"), &["-zsh"])
            },
        ];
        let identity = IdentityMap {
            entities: [
                (
                    "bundle:test".to_owned(),
                    EntitySeed {
                        entity_id: "bundle:test".to_owned(),
                        display_name: "Test".to_owned(),
                        bundle_id: Some("local.test".to_owned()),
                        executable_path: Some(
                            "/Applications/Test.app/Contents/MacOS/Test".to_owned(),
                        ),
                        entity_kind: EntityKind::App,
                        badges: Vec::new(),
                    },
                ),
                (
                    "process:launchd".to_owned(),
                    EntitySeed {
                        entity_id: "process:launchd".to_owned(),
                        display_name: "launchd".to_owned(),
                        bundle_id: None,
                        executable_path: Some("/sbin/launchd".to_owned()),
                        entity_kind: EntityKind::Service,
                        badges: Vec::new(),
                    },
                ),
                (
                    "process:zsh".to_owned(),
                    EntitySeed {
                        entity_id: "process:zsh".to_owned(),
                        display_name: "zsh".to_owned(),
                        bundle_id: None,
                        executable_path: Some("/bin/zsh".to_owned()),
                        entity_kind: EntityKind::TerminalSession,
                        badges: Vec::new(),
                    },
                ),
            ]
            .into_iter()
            .collect(),
            pid_to_entity: [
                (10, "bundle:test".to_owned()),
                (11, "bundle:test".to_owned()),
                (1, "process:launchd".to_owned()),
                (2, "process:zsh".to_owned()),
            ]
            .into_iter()
            .collect(),
        };

        let entities = build_entities(&processes, &identity, None);
        let entity = entities
            .iter()
            .find(|value| value.entity_id == "bundle:test")
            .expect("test entity");

        assert!(
            entity
                .launcher_summary
                .as_deref()
                .is_some_and(|summary| summary.contains("mixed lineage"))
        );
        assert!(!entity.attribution_notes.is_empty());
        assert_eq!(
            entity
                .primary_provenance
                .as_ref()
                .map(|value| &value.confidence),
            Some(&AttributionConfidence::Medium)
        );
    }
}
