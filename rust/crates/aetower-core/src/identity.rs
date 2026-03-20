use std::collections::BTreeMap;

use aetower_model::EntityKind;

use crate::collector::{index_processes, RawProcessSample};

#[derive(Debug, Clone)]
pub struct EntitySeed {
    pub entity_id: String,
    pub display_name: String,
    pub bundle_id: Option<String>,
    pub executable_path: Option<String>,
    pub entity_kind: EntityKind,
    pub badges: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct IdentityMap {
    pub entities: BTreeMap<String, EntitySeed>,
    pub pid_to_entity: BTreeMap<u32, String>,
}

pub fn resolve(processes: &[RawProcessSample]) -> IdentityMap {
    let process_index = index_processes(processes);
    let mut map = IdentityMap::default();

    for process in processes {
        let seed = classify_process(process, &process_index);
        map.pid_to_entity.insert(process.pid, seed.entity_id.clone());
        map.entities.entry(seed.entity_id.clone()).or_insert(seed);
    }

    map
}

fn classify_process(
    process: &RawProcessSample,
    process_index: &BTreeMap<u32, &RawProcessSample>,
) -> EntitySeed {
    if let Some(exe) = process.exe.as_deref() {
        if let Some(bundle_name) = extract_app_bundle_name(exe) {
            let browser = is_browser_name(&bundle_name);
            return EntitySeed {
                entity_id: format!("bundle:{}", bundle_name.to_lowercase()),
                display_name: bundle_name.clone(),
                bundle_id: Some(format!("local.{}", bundle_name.to_lowercase().replace(' ', "-"))),
                executable_path: Some(exe.to_owned()),
                entity_kind: if browser {
                    EntityKind::Browser
                } else {
                    EntityKind::App
                },
                badges: default_badges(exe, browser),
            };
        }
    }

    if is_shell(&process.name) {
        let command = process
            .cmd
            .iter()
            .skip(1)
            .find(|segment| !segment.starts_with('-'))
            .cloned()
            .unwrap_or_else(|| process.name.clone());
        return EntitySeed {
            entity_id: format!("terminal:{}", process.pid),
            display_name: command.clone(),
            bundle_id: None,
            executable_path: process.exe.clone(),
            entity_kind: EntityKind::TerminalSession,
            badges: vec!["interactive".to_owned()],
        };
    }

    if let Some(parent_pid) = process.parent_pid {
        if let Some(parent) = process_index.get(&parent_pid) {
            if let Some(parent_exe) = parent.exe.as_deref() {
                if let Some(parent_bundle) = extract_app_bundle_name(parent_exe) {
                    return EntitySeed {
                        entity_id: format!("bundle:{}", parent_bundle.to_lowercase()),
                        display_name: parent_bundle.clone(),
                        bundle_id: Some(format!(
                            "local.{}",
                            parent_bundle.to_lowercase().replace(' ', "-")
                        )),
                        executable_path: Some(parent_exe.to_owned()),
                        entity_kind: if is_browser_name(&parent_bundle) {
                            EntityKind::Browser
                        } else {
                            EntityKind::App
                        },
                        badges: vec!["helper-group".to_owned()],
                    };
                }
            }
        }
    }

    let daemon = process
        .exe
        .as_deref()
        .map(|path| path.starts_with("/System/Library") || path.starts_with("/usr/libexec"))
        .unwrap_or(false);

    EntitySeed {
        entity_id: format!("process:{}", normalized_name(&process.name)),
        display_name: process.name.clone(),
        bundle_id: None,
        executable_path: process.exe.clone(),
        entity_kind: if daemon {
            EntityKind::Daemon
        } else {
            EntityKind::Service
        },
        badges: default_badges(process.exe.as_deref().unwrap_or_default(), false),
    }
}

fn extract_app_bundle_name(path: &str) -> Option<String> {
    let marker = ".app/";
    let index = path.find(marker)?;
    let prefix = &path[..index + 4];
    let name = prefix.rsplit('/').next()?.trim_end_matches(".app");
    if name.is_empty() {
        None
    } else {
        Some(name.to_owned())
    }
}

fn is_shell(name: &str) -> bool {
    matches!(name, "zsh" | "bash" | "fish" | "sh")
}

fn is_browser_name(name: &str) -> bool {
    matches!(name, "Google Chrome" | "Chromium" | "Microsoft Edge" | "Brave Browser" | "Arc")
}

fn normalized_name(name: &str) -> String {
    name.to_lowercase().replace(' ', "-")
}

fn default_badges(path: &str, browser: bool) -> Vec<String> {
    let mut badges = Vec::new();
    if browser {
        badges.push("browser".to_owned());
    }
    if path.starts_with("/Applications/") {
        badges.push("user-app".to_owned());
    }
    if path.starts_with("/System/") {
        badges.push("system".to_owned());
    }
    badges
}
