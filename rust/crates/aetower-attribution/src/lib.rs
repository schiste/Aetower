use std::collections::BTreeMap;

use aetower_collector::{RawProcessSample, index_processes};
use aetower_identity::{EntitySeed, IdentityMap};
use aetower_model::{
    AggregateMetrics, ComponentKind, ComponentSnapshot, EntityKind, EntitySnapshot,
    FrontmostAppState, ProvenanceKind, ProvenanceSnapshot,
};

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
        entry.metrics.disk_read_bps = entry
            .metrics
            .disk_read_bps
            .saturating_add(process.disk_read_bytes);
        entry.metrics.disk_write_bps = entry
            .metrics
            .disk_write_bps
            .saturating_add(process.disk_write_bytes);
        entry.metrics.wakeups_per_second += process.wakeups_per_second;
        entry.metrics.process_count += 1;
        entry.metrics.is_foreground =
            entry.metrics.is_foreground || is_foreground_match(seed, frontmost);
        if entry.metrics.is_foreground {
            entry.active_window_title = frontmost.and_then(|state| state.window_title.clone());
        }

        entry.components.push(ComponentSnapshot {
            kind: if seed.entity_kind == EntityKind::TerminalSession {
                ComponentKind::Command
            } else {
                ComponentKind::Process
            },
            title: process.name.clone(),
            detail: summarize_process(process),
            provenance: component_provenance(process, &seed.entity_id, &process_index, identity),
            process_id: Some(process.pid),
            executable_path: process.exe.clone(),
            command_line: command_line(process),
            parent_summary: parent_summary(process, &process_index),
            launched_by: launched_by(process, &seed.entity_id, &process_index, identity),
            cpu_percent: process.cpu_percent,
            memory_bytes: process.memory_bytes,
            cwd: process.cwd.clone(),
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
    }
    entities
}

fn entity_from_seed(seed: &EntitySeed) -> EntitySnapshot {
    EntitySnapshot {
        entity_id: seed.entity_id.clone(),
        display_name: seed.display_name.clone(),
        primary_provenance: entity_provenance(seed),
        bundle_id: seed.bundle_id.clone(),
        executable_path: seed.executable_path.clone(),
        oldest_process_start_millis: 0,
        newest_process_start_millis: 0,
        entity_kind: seed.entity_kind.clone(),
        metrics: AggregateMetrics::default(),
        friction: Default::default(),
        components: Vec::new(),
        trend: Default::default(),
        badges: seed.badges.clone(),
        active_window_title: None,
        anomaly_detected: false,
        thermal_contribution: None,
        grouping_suggestion: None,
        agent_cost: None,
        session_markers: Vec::new(),
        recommendations: Vec::new(),
    }
}

fn entity_provenance(seed: &EntitySeed) -> Option<ProvenanceSnapshot> {
    if seed.badges.iter().any(|badge| badge == "user-launch") {
        return Some(provenance(ProvenanceKind::UserLaunch, "User-launched app"));
    }
    if seed.badges.iter().any(|badge| badge == "xpc-service") {
        return Some(provenance(ProvenanceKind::XpcService, "XPC service"));
    }
    if seed.entity_kind == EntityKind::TerminalSession {
        return Some(provenance(
            ProvenanceKind::ShellSession,
            "Interactive shell session",
        ));
    }
    if seed.badges.iter().any(|badge| badge == "login-item") {
        return Some(provenance(ProvenanceKind::LoginItem, "Login item"));
    }
    if seed.badges.iter().any(|badge| badge == "launchd-managed") {
        return Some(provenance(
            ProvenanceKind::ServiceManager,
            "launchd-managed service",
        ));
    }
    if seed.badges.iter().any(|badge| badge == "helper-group") {
        return Some(provenance(
            ProvenanceKind::HelperTree,
            "Grouped app helper tree",
        ));
    }
    if seed.bundle_id.is_some() || seed.entity_kind == EntityKind::App {
        return Some(provenance(ProvenanceKind::AppBundle, "Application bundle"));
    }
    None
}

fn summarize_process(process: &RawProcessSample) -> String {
    if process.cmd.is_empty() {
        format!("pid {}", process.pid)
    } else {
        process.cmd.join(" ")
    }
}

fn command_line(process: &RawProcessSample) -> Option<String> {
    (!process.cmd.is_empty()).then(|| process.cmd.join(" "))
}

fn parent_summary(
    process: &RawProcessSample,
    process_index: &BTreeMap<u32, &RawProcessSample>,
) -> Option<String> {
    let parent_pid = process.parent_pid?;
    let parent = process_index.get(&parent_pid)?;
    Some(format_process_label(parent))
}

fn launched_by(
    process: &RawProcessSample,
    entity_id: &str,
    process_index: &BTreeMap<u32, &RawProcessSample>,
    identity: &IdentityMap,
) -> Option<String> {
    let mut next_pid = process.parent_pid;
    let mut immediate_parent: Option<&RawProcessSample> = None;
    let mut top_same_entity_ancestor: Option<&RawProcessSample> = None;
    let mut shell_ancestor: Option<&RawProcessSample> = None;
    let mut login_item_ancestor: Option<&RawProcessSample> = None;
    let mut service_manager_ancestor: Option<&RawProcessSample> = None;

    while let Some(pid) = next_pid {
        let Some(ancestor) = process_index.get(&pid).copied() else {
            break;
        };
        if immediate_parent.is_none() {
            immediate_parent = Some(ancestor);
        }
        if identity
            .pid_to_entity
            .get(&ancestor.pid)
            .is_some_and(|ancestor_entity_id| ancestor_entity_id == entity_id)
        {
            top_same_entity_ancestor = Some(ancestor);
        }
        if shell_ancestor.is_none() && is_shell_process(ancestor) {
            shell_ancestor = Some(ancestor);
        }
        if login_item_ancestor.is_none() && is_login_item_launcher(ancestor) {
            login_item_ancestor = Some(ancestor);
        }
        if service_manager_ancestor.is_none() && is_service_manager(ancestor) {
            service_manager_ancestor = Some(ancestor);
        }
        next_pid = ancestor.parent_pid;
    }

    shell_ancestor
        .map(format_shell_session_label)
        .or_else(|| top_same_entity_ancestor.map(format_process_label))
        .or_else(|| login_item_ancestor.map(format_login_item_label))
        .or_else(|| service_manager_ancestor.map(format_service_manager_label))
        .or_else(|| immediate_parent.map(format_process_label))
}

fn component_provenance(
    process: &RawProcessSample,
    entity_id: &str,
    process_index: &BTreeMap<u32, &RawProcessSample>,
    identity: &IdentityMap,
) -> Option<ProvenanceSnapshot> {
    if process
        .exe
        .as_deref()
        .is_some_and(|path| path.contains(".xpc/") || path.contains("/Contents/XPCServices/"))
    {
        return Some(provenance(
            ProvenanceKind::XpcService,
            "XPC service process",
        ));
    }

    let mut next_pid = process.parent_pid;
    let mut top_same_entity_ancestor: Option<&RawProcessSample> = None;
    let mut shell_ancestor: Option<&RawProcessSample> = None;
    let mut login_item_ancestor: Option<&RawProcessSample> = None;
    let mut service_manager_ancestor: Option<&RawProcessSample> = None;
    let mut immediate_parent: Option<&RawProcessSample> = None;

    while let Some(pid) = next_pid {
        let Some(ancestor) = process_index.get(&pid).copied() else {
            break;
        };
        if immediate_parent.is_none() {
            immediate_parent = Some(ancestor);
        }
        if identity
            .pid_to_entity
            .get(&ancestor.pid)
            .is_some_and(|ancestor_entity_id| ancestor_entity_id == entity_id)
        {
            top_same_entity_ancestor = Some(ancestor);
        }
        if shell_ancestor.is_none() && is_shell_process(ancestor) {
            shell_ancestor = Some(ancestor);
        }
        if login_item_ancestor.is_none() && is_login_item_launcher(ancestor) {
            login_item_ancestor = Some(ancestor);
        }
        if service_manager_ancestor.is_none() && is_service_manager(ancestor) {
            service_manager_ancestor = Some(ancestor);
        }
        next_pid = ancestor.parent_pid;
    }

    if let Some(shell) = shell_ancestor {
        Some(provenance(
            ProvenanceKind::ShellSession,
            &format!("Shell session via {}", process_display_name(shell)),
        ))
    } else if top_same_entity_ancestor.is_some() {
        Some(provenance(
            ProvenanceKind::HelperTree,
            "App helper subprocess",
        ))
    } else if let Some(login_item) = login_item_ancestor {
        Some(provenance(
            ProvenanceKind::LoginItem,
            &format!("Login item via {}", process_display_name(login_item)),
        ))
    } else if service_manager_ancestor.is_some() {
        Some(provenance(
            ProvenanceKind::ServiceManager,
            "launchd service manager",
        ))
    } else {
        immediate_parent.map(|parent| {
            provenance(
                ProvenanceKind::ParentProcess,
                &format!("Parent process {}", process_display_name(parent)),
            )
        })
    }
}

fn format_process_label(process: &RawProcessSample) -> String {
    format!("{} (pid {})", process_display_name(process), process.pid)
}

fn process_display_name(process: &RawProcessSample) -> String {
    if !process.name.is_empty() {
        process.name.clone()
    } else if let Some(path) = process.exe.as_deref() {
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
    matches!(process.name.as_str(), "zsh" | "bash" | "fish" | "sh")
}

fn is_login_item_launcher(process: &RawProcessSample) -> bool {
    matches!(process.name.as_str(), "loginwindow" | "xpcproxy")
}

fn is_service_manager(process: &RawProcessSample) -> bool {
    process.name == "launchd"
}

fn format_shell_session_label(process: &RawProcessSample) -> String {
    format!(
        "{} shell session (pid {})",
        process_display_name(process),
        process.pid
    )
}

fn format_login_item_label(process: &RawProcessSample) -> String {
    match process.name.as_str() {
        "xpcproxy" => format!("xpcproxy login item launcher (pid {})", process.pid),
        "loginwindow" => format!("loginwindow session launcher (pid {})", process.pid),
        _ => format_process_label(process),
    }
}

fn format_service_manager_label(process: &RawProcessSample) -> String {
    format!("launchd service manager (pid {})", process.pid)
}

fn provenance(kind: ProvenanceKind, label: &str) -> ProvenanceSnapshot {
    ProvenanceSnapshot {
        kind,
        label: label.to_owned(),
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
                pid: 1,
                parent_pid: None,
                start_time_millis: 10,
                name: "Renderer".to_owned(),
                exe: Some("/Applications/Test.app/Contents/MacOS/Test".to_owned()),
                cmd: Vec::new(),
                cpu_percent: 10.0,
                memory_bytes: 128,

                disk_read_bytes: 100,
                disk_write_bytes: 200,
                wakeups_per_second: 0.0,
                cwd: None,
            },
            RawProcessSample {
                pid: 2,
                parent_pid: Some(1),
                start_time_millis: 20,
                name: "Helper".to_owned(),
                exe: Some("/Applications/Test.app/Contents/MacOS/Test Helper".to_owned()),
                cmd: Vec::new(),
                cpu_percent: 5.0,
                memory_bytes: 256,

                disk_read_bytes: 300,
                disk_write_bytes: 400,
                wakeups_per_second: 0.0,
                cwd: None,
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
                pid: 10,
                parent_pid: Some(1),
                start_time_millis: 10,
                name: "Test".to_owned(),
                exe: Some("/Applications/Test.app/Contents/MacOS/Test".to_owned()),
                cmd: vec!["/Applications/Test.app/Contents/MacOS/Test".to_owned()],
                cpu_percent: 1.0,
                memory_bytes: 128,

                disk_read_bytes: 0,
                disk_write_bytes: 0,
                wakeups_per_second: 0.0,
                cwd: None,
            },
            RawProcessSample {
                pid: 11,
                parent_pid: Some(10),
                start_time_millis: 20,
                name: "Test Helper".to_owned(),
                exe: Some("/Applications/Test.app/Contents/MacOS/Test Helper".to_owned()),
                cmd: vec![
                    "/Applications/Test.app/Contents/MacOS/Test Helper".to_owned(),
                    "--type=renderer".to_owned(),
                ],
                cpu_percent: 5.0,
                memory_bytes: 256,

                disk_read_bytes: 0,
                disk_write_bytes: 0,
                wakeups_per_second: 0.0,
                cwd: None,
            },
            RawProcessSample {
                pid: 1,
                parent_pid: None,
                start_time_millis: 1,
                name: "launchd".to_owned(),
                exe: Some("/sbin/launchd".to_owned()),
                cmd: vec!["/sbin/launchd".to_owned()],
                cpu_percent: 0.0,
                memory_bytes: 0,

                disk_read_bytes: 0,
                disk_write_bytes: 0,
                wakeups_per_second: 0.0,
                cwd: None,
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
    }

    #[test]
    fn process_component_prefers_shell_session_for_external_launcher() {
        let processes = vec![
            RawProcessSample {
                pid: 20,
                parent_pid: Some(10),
                start_time_millis: 10,
                name: "zsh".to_owned(),
                exe: Some("/bin/zsh".to_owned()),
                cmd: vec!["-zsh".to_owned()],
                cpu_percent: 0.0,
                memory_bytes: 0,

                disk_read_bytes: 0,
                disk_write_bytes: 0,
                wakeups_per_second: 0.0,
                cwd: None,
            },
            RawProcessSample {
                pid: 21,
                parent_pid: Some(20),
                start_time_millis: 20,
                name: "python3".to_owned(),
                exe: Some("/opt/homebrew/bin/python3".to_owned()),
                cmd: vec!["python3".to_owned(), "server.py".to_owned()],
                cpu_percent: 1.0,
                memory_bytes: 64,

                disk_read_bytes: 0,
                disk_write_bytes: 0,
                wakeups_per_second: 0.0,
                cwd: None,
            },
            RawProcessSample {
                pid: 10,
                parent_pid: None,
                start_time_millis: 1,
                name: "Terminal".to_owned(),
                exe: Some(
                    "/System/Applications/Utilities/Terminal.app/Contents/MacOS/Terminal"
                        .to_owned(),
                ),
                cmd: vec!["Terminal".to_owned()],
                cpu_percent: 0.0,
                memory_bytes: 0,

                disk_read_bytes: 0,
                disk_write_bytes: 0,
                wakeups_per_second: 0.0,
                cwd: None,
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
                pid: 31,
                parent_pid: Some(1),
                start_time_millis: 20,
                name: "sync-agent".to_owned(),
                exe: Some("/usr/libexec/sync-agent".to_owned()),
                cmd: vec!["/usr/libexec/sync-agent".to_owned()],
                cpu_percent: 1.0,
                memory_bytes: 64,

                disk_read_bytes: 0,
                disk_write_bytes: 0,
                wakeups_per_second: 0.0,
                cwd: None,
            },
            RawProcessSample {
                pid: 1,
                parent_pid: None,
                start_time_millis: 1,
                name: "launchd".to_owned(),
                exe: Some("/sbin/launchd".to_owned()),
                cmd: vec!["/sbin/launchd".to_owned()],
                cpu_percent: 0.0,
                memory_bytes: 0,

                disk_read_bytes: 0,
                disk_write_bytes: 0,
                wakeups_per_second: 0.0,
                cwd: None,
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
                pid: 41,
                parent_pid: Some(3),
                start_time_millis: 20,
                name: "MenuBarExtra".to_owned(),
                exe: Some("/Users/test/Library/Application Support/Foo/MenuBarExtra".to_owned()),
                cmd: vec!["MenuBarExtra".to_owned()],
                cpu_percent: 1.0,
                memory_bytes: 64,

                disk_read_bytes: 0,
                disk_write_bytes: 0,
                wakeups_per_second: 0.0,
                cwd: None,
            },
            RawProcessSample {
                pid: 3,
                parent_pid: Some(2),
                start_time_millis: 10,
                name: "xpcproxy".to_owned(),
                exe: Some("/usr/libexec/xpcproxy".to_owned()),
                cmd: vec!["/usr/libexec/xpcproxy".to_owned()],
                cpu_percent: 0.0,
                memory_bytes: 0,

                disk_read_bytes: 0,
                disk_write_bytes: 0,
                wakeups_per_second: 0.0,
                cwd: None,
            },
            RawProcessSample {
                pid: 2,
                parent_pid: Some(1),
                start_time_millis: 5,
                name: "loginwindow".to_owned(),
                exe: Some(
                    "/System/Library/CoreServices/loginwindow.app/Contents/MacOS/loginwindow"
                        .to_owned(),
                ),
                cmd: vec!["loginwindow".to_owned()],
                cpu_percent: 0.0,
                memory_bytes: 0,

                disk_read_bytes: 0,
                disk_write_bytes: 0,
                wakeups_per_second: 0.0,
                cwd: None,
            },
            RawProcessSample {
                pid: 1,
                parent_pid: None,
                start_time_millis: 1,
                name: "launchd".to_owned(),
                exe: Some("/sbin/launchd".to_owned()),
                cmd: vec!["/sbin/launchd".to_owned()],
                cpu_percent: 0.0,
                memory_bytes: 0,

                disk_read_bytes: 0,
                disk_write_bytes: 0,
                wakeups_per_second: 0.0,
                cwd: None,
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
}
