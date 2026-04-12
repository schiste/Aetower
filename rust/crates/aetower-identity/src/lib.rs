use std::collections::BTreeMap;

use aetower_collector::{RawProcessSample, index_processes};
use aetower_model::EntityKind;

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
        map.pid_to_entity
            .insert(process.pid, seed.entity_id.clone());
        map.entities
            .entry(seed.entity_id.clone())
            .and_modify(|existing| merge_seed(existing, &seed))
            .or_insert(seed);
    }

    map
}

fn merge_seed(existing: &mut EntitySeed, candidate: &EntitySeed) {
    if existing.bundle_id.is_none() {
        existing.bundle_id = candidate.bundle_id.clone();
    }
    if existing.executable_path.is_none() {
        existing.executable_path = candidate.executable_path.clone();
    }
    if existing.display_name == "Unknown Process" && candidate.display_name != "Unknown Process" {
        existing.display_name = candidate.display_name.clone();
    }
    for badge in &candidate.badges {
        if !existing
            .badges
            .iter()
            .any(|existing_badge| existing_badge == badge)
        {
            existing.badges.push(badge.clone());
        }
    }
}

fn classify_process(
    process: &RawProcessSample,
    process_index: &BTreeMap<u32, &RawProcessSample>,
) -> EntitySeed {
    if let Some(shell_process) = terminal_shell_process(process, process_index) {
        if is_ai_agent_exe(&process.name) {
            return EntitySeed {
                entity_id: format!("ai-agent:{}", shell_process.pid),
                display_name: ai_agent_display_name(process),
                bundle_id: None,
                executable_path: process.exe.clone(),
                entity_kind: EntityKind::AiAgent,
                badges: vec!["ai-agent".to_owned(), "shell-tree".to_owned()],
            };
        }
        return terminal_session_seed(shell_process);
    }

    // Daemon-launched AI/ML inference servers (ollama, LM Studio, llama.cpp
    // via homebrew service, MLX server, etc.) run as standalone processes
    // under launchd, not inside a terminal shell. Classify them by process
    // name alone — the name list is specific enough that false positives
    // are vanishingly unlikely. These get entity_id keyed on their
    // executable path so each distinct server binary is its own entity.
    if is_ai_agent_exe(&process.name) {
        let entity_id = process
            .exe
            .as_deref()
            .map(|path| format!("ai-agent-daemon:{}", path.to_lowercase()))
            .unwrap_or_else(|| format!("ai-agent:{}", normalized_name(&process.name)));
        return EntitySeed {
            entity_id,
            display_name: ai_agent_display_name(process),
            bundle_id: None,
            executable_path: process.exe.clone(),
            entity_kind: EntityKind::AiAgent,
            badges: vec!["ai-agent".to_owned(), "daemon".to_owned()],
        };
    }

    if let Some(seed) = app_bundle_seed(process, process_index) {
        return seed;
    }

    let daemon = process
        .exe
        .as_deref()
        .map(|path| path.starts_with("/System/Library") || path.starts_with("/usr/libexec"))
        .unwrap_or(false);

    let launchd_managed = has_lineage_match(process, process_index, |ancestor| {
        ancestor.name == "launchd"
    });
    let login_item = has_lineage_match(process, process_index, |ancestor| {
        ancestor.name == "loginwindow" || ancestor.name == "xpcproxy"
    });
    let xpc_service = is_xpc_service_path(process.exe.as_deref().unwrap_or_default());
    let display_name = process_display_name(process);
    let entity_id = process
        .exe
        .as_deref()
        .map(|path| format!("process-path:{}", path.to_lowercase()))
        .unwrap_or_else(|| format!("process:{}", normalized_name(&display_name)));
    let mut badges = default_badges(process.exe.as_deref().unwrap_or_default(), false);
    if launchd_managed {
        badges.push("launchd-managed".to_owned());
    }
    if login_item {
        badges.push("login-item".to_owned());
    }
    if xpc_service {
        badges.push("xpc-service".to_owned());
    }

    EntitySeed {
        entity_id,
        display_name,
        bundle_id: None,
        executable_path: process.exe.clone(),
        entity_kind: if daemon {
            EntityKind::Daemon
        } else {
            EntityKind::Service
        },
        badges,
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
    matches!(
        name,
        "Google Chrome" | "Chromium" | "Microsoft Edge" | "Brave Browser" | "Arc" | "Safari"
    )
}

/// Match process executable names to known AI/ML inference tools.
///
/// The list covers three categories:
/// - **AI coding agents**: claude, codex, aider, cursor-agent
/// - **Local LLM servers**: ollama, llama.cpp, MLX, LM Studio, koboldcpp, llamafile
/// - **Local ML inference**: whisper.cpp, Stable Diffusion, ComfyUI
///
/// Names are matched case-sensitively against `RawProcessSample.name`
/// which is derived from `sysinfo::Process::name()` — on macOS this
/// is the binary file name, not the bundle display name.
fn is_ai_agent_exe(name: &str) -> bool {
    matches!(
        name,
        // AI coding agents
        "claude"
            | "claude-code"
            | "codex"
            | "aider"
            | "cursor-agent"
            // Local LLM servers
            | "ollama"
            | "ollama-runner"
            | "ollama_llama_server"
            | "mlx_lm"
            | "mlx-lm-server"
            | "mlx_lm.server"
            | "llama-server"
            | "llama-cli"
            | "llama-bench"
            | "llamafile"
            | "lm-studio"
            | "lmstudio"
            | "LM Studio"
            | "koboldcpp"
            // Local ML inference
            | "whisper"
            | "whisper-server"
            | "whisper-cpp"
    )
}

fn ai_agent_display_name(process: &RawProcessSample) -> String {
    match process.name.as_str() {
        "claude" | "claude-code" => "Claude Code".to_owned(),
        "codex" => "Codex".to_owned(),
        "aider" => "Aider".to_owned(),
        "cursor-agent" => "Cursor Agent".to_owned(),
        "ollama" | "ollama-runner" | "ollama_llama_server" => "Ollama".to_owned(),
        "mlx_lm" | "mlx-lm-server" | "mlx_lm.server" => "MLX Language Model".to_owned(),
        "llama-server" | "llama-cli" | "llama-bench" => "llama.cpp".to_owned(),
        "llamafile" => "Llamafile".to_owned(),
        "lm-studio" | "lmstudio" | "LM Studio" => "LM Studio".to_owned(),
        "koboldcpp" => "KoboldCpp".to_owned(),
        "whisper" | "whisper-server" | "whisper-cpp" => "Whisper".to_owned(),
        other => other.to_owned(),
    }
}

fn normalized_name(name: &str) -> String {
    name.to_lowercase().replace(' ', "-")
}

fn process_display_name(process: &RawProcessSample) -> String {
    if !process.name.is_empty() {
        process.name.clone()
    } else if let Some(exe) = process.exe.as_deref() {
        exe.rsplit('/').next().unwrap_or(exe).to_owned()
    } else {
        "Unknown Process".to_owned()
    }
}

fn app_bundle_seed(
    process: &RawProcessSample,
    process_index: &BTreeMap<u32, &RawProcessSample>,
) -> Option<EntitySeed> {
    let bundle_process = root_bundle_process(process, process_index)?;
    let bundle_path = bundle_process.exe.as_deref()?;
    let bundle_name = extract_app_bundle_name(bundle_path)?;
    let browser = is_browser_name(&bundle_name);
    let helper_group = bundle_process.pid != process.pid || is_bundle_helper(process);
    let mut badges = default_badges(bundle_path, browser);
    if helper_group {
        badges.push("helper-group".to_owned());
    }
    if is_user_launch(bundle_process, process_index) {
        badges.push("user-launch".to_owned());
    }

    Some(EntitySeed {
        entity_id: format!(
            "bundle-path:{}",
            bundle_root_path(bundle_path).to_lowercase()
        ),
        display_name: bundle_name.clone(),
        bundle_id: Some(format!(
            "local.{}",
            bundle_name.to_lowercase().replace(' ', "-")
        )),
        executable_path: Some(bundle_path.to_owned()),
        entity_kind: if browser {
            EntityKind::Browser
        } else {
            EntityKind::App
        },
        badges,
    })
}

fn terminal_shell_process<'a>(
    process: &'a RawProcessSample,
    process_index: &'a BTreeMap<u32, &RawProcessSample>,
) -> Option<&'a RawProcessSample> {
    if is_shell(&process.name) {
        return Some(process);
    }

    let mut current = process
        .parent_pid
        .and_then(|pid| process_index.get(&pid).copied());
    while let Some(candidate) = current {
        if is_shell(&candidate.name) {
            return Some(candidate);
        }
        current = candidate
            .parent_pid
            .and_then(|parent_pid| process_index.get(&parent_pid).copied());
    }

    None
}

fn terminal_session_seed(shell: &RawProcessSample) -> EntitySeed {
    let display_name = shell_session_title(shell);
    EntitySeed {
        entity_id: format!("terminal:{}", shell.pid),
        display_name,
        bundle_id: None,
        executable_path: shell.exe.clone(),
        entity_kind: EntityKind::TerminalSession,
        badges: vec!["interactive".to_owned(), "shell-tree".to_owned()],
    }
}

fn shell_session_title(shell: &RawProcessSample) -> String {
    shell
        .cmd
        .iter()
        .skip(1)
        .find(|segment| !segment.starts_with('-'))
        .cloned()
        .filter(|segment| segment != &shell.name)
        .unwrap_or_else(|| format!("{} session", shell.name))
}

fn root_bundle_process<'a>(
    process: &'a RawProcessSample,
    process_index: &'a BTreeMap<u32, &RawProcessSample>,
) -> Option<&'a RawProcessSample> {
    let process_path = process.exe.as_deref()?;
    extract_app_bundle_name(process_path)?;
    let process_bundle_root = bundle_root_path(process_path);
    let mut candidate = Some(process);
    let mut current = process
        .parent_pid
        .and_then(|pid| process_index.get(&pid).copied());

    while let Some(ancestor) = current {
        if ancestor.exe.as_deref().is_some_and(|path| {
            extract_app_bundle_name(path).is_some() && bundle_root_path(path) == process_bundle_root
        }) {
            candidate = Some(ancestor);
        }
        current = ancestor
            .parent_pid
            .and_then(|parent_pid| process_index.get(&parent_pid).copied());
    }

    candidate
}

fn bundle_root_path(path: &str) -> &str {
    let marker = ".app/";
    path.find(marker)
        .map(|index| &path[..index + 4])
        .unwrap_or(path)
}

fn is_bundle_helper(process: &RawProcessSample) -> bool {
    process.exe.as_deref().is_some_and(|path| {
        path.contains("/Contents/Frameworks/")
            || path.contains("/Contents/Helpers/")
            || path.contains("/Contents/XPCServices/")
    })
}

fn is_xpc_service_path(path: &str) -> bool {
    path.contains(".xpc/") || path.contains("/Contents/XPCServices/")
}

fn has_lineage_match(
    process: &RawProcessSample,
    process_index: &BTreeMap<u32, &RawProcessSample>,
    predicate: impl Fn(&RawProcessSample) -> bool,
) -> bool {
    let mut current = process
        .parent_pid
        .and_then(|pid| process_index.get(&pid).copied());
    while let Some(ancestor) = current {
        if predicate(ancestor) {
            return true;
        }
        current = ancestor
            .parent_pid
            .and_then(|parent_pid| process_index.get(&parent_pid).copied());
    }
    false
}

fn is_user_launch(
    process: &RawProcessSample,
    process_index: &BTreeMap<u32, &RawProcessSample>,
) -> bool {
    process
        .parent_pid
        .and_then(|pid| process_index.get(&pid).copied())
        .is_some_and(|parent| {
            matches!(
                parent.name.as_str(),
                "Dock" | "Finder" | "launchservicesd" | "LaunchServices"
            )
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(
        pid: u32,
        parent_pid: Option<u32>,
        name: &str,
        exe: Option<&str>,
        cmd: &[&str],
    ) -> RawProcessSample {
        RawProcessSample {
            pid,
            parent_pid,
            start_time_millis: 0,
            name: name.to_owned(),
            exe: exe.map(str::to_owned),
            cmd: cmd.iter().map(|value| (*value).to_owned()).collect(),
            cpu_percent: 0.0,
            memory_bytes: 0,
            disk_read_bytes: 0,
            disk_write_bytes: 0,
            wakeups_per_second: 0.0,
            energy_nj_per_s: 0.0,
            cwd: None,
            user: None,
        }
    }

    #[test]
    fn groups_nested_helper_into_root_app_bundle() {
        let processes = vec![
            sample(
                1,
                None,
                "launchd",
                Some("/sbin/launchd"),
                &["/sbin/launchd"],
            ),
            sample(
                2,
                Some(1),
                "Dock",
                Some("/System/Library/CoreServices/Dock.app/Contents/MacOS/Dock"),
                &["Dock"],
            ),
            sample(
                100,
                Some(2),
                "Google Chrome",
                Some("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"),
                &["/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"],
            ),
            sample(
                101,
                Some(100),
                "Google Chrome Helper",
                Some(
                    "/Applications/Google Chrome.app/Contents/Frameworks/Google Chrome Helper.app/Contents/MacOS/Google Chrome Helper",
                ),
                &["helper"],
            ),
        ];

        let identity = resolve(&processes);
        let entity_id = identity
            .pid_to_entity
            .get(&101)
            .expect("helper should resolve");
        let entity = identity
            .entities
            .get(entity_id)
            .expect("entity should exist");

        assert_eq!(entity.display_name, "Google Chrome");
        assert!(entity.badges.iter().any(|badge| badge == "helper-group"));
        assert!(entity.badges.iter().any(|badge| badge == "user-launch"));
        assert_eq!(
            entity.entity_id,
            "bundle-path:/applications/google chrome.app"
        );
    }

    #[test]
    fn shell_descendant_stays_in_terminal_session() {
        let processes = vec![
            sample(
                1,
                None,
                "launchd",
                Some("/sbin/launchd"),
                &["/sbin/launchd"],
            ),
            sample(
                10,
                Some(1),
                "Terminal",
                Some("/System/Applications/Utilities/Terminal.app/Contents/MacOS/Terminal"),
                &["Terminal"],
            ),
            sample(20, Some(10), "zsh", Some("/bin/zsh"), &["-zsh"]),
            sample(
                21,
                Some(20),
                "python3",
                Some("/opt/homebrew/bin/python3"),
                &["python3", "server.py"],
            ),
        ];

        let identity = resolve(&processes);
        let entity_id = identity
            .pid_to_entity
            .get(&21)
            .expect("shell child should resolve");
        let entity = identity
            .entities
            .get(entity_id)
            .expect("entity should exist");

        assert_eq!(entity.entity_kind, EntityKind::TerminalSession);
        assert_eq!(entity.entity_id, "terminal:20");
        assert_eq!(entity.display_name, "zsh session");
    }

    #[test]
    fn launchd_service_gets_launchd_managed_badge() {
        let processes = vec![
            sample(
                1,
                None,
                "launchd",
                Some("/sbin/launchd"),
                &["/sbin/launchd"],
            ),
            sample(
                200,
                Some(1),
                "sysmond",
                Some("/usr/libexec/sysmond"),
                &["/usr/libexec/sysmond"],
            ),
        ];

        let identity = resolve(&processes);
        let entity = identity
            .entities
            .get(
                identity
                    .pid_to_entity
                    .get(&200)
                    .expect("service should resolve"),
            )
            .expect("entity should exist");

        assert_eq!(entity.entity_kind, EntityKind::Daemon);
        assert!(entity.badges.iter().any(|badge| badge == "launchd-managed"));
    }

    #[test]
    fn login_item_service_gets_login_item_badge() {
        let processes = vec![
            sample(
                1,
                None,
                "launchd",
                Some("/sbin/launchd"),
                &["/sbin/launchd"],
            ),
            sample(
                2,
                Some(1),
                "loginwindow",
                Some("/System/Library/CoreServices/loginwindow.app/Contents/MacOS/loginwindow"),
                &["loginwindow"],
            ),
            sample(
                3,
                Some(2),
                "xpcproxy",
                Some("/usr/libexec/xpcproxy"),
                &["/usr/libexec/xpcproxy"],
            ),
            sample(
                4,
                Some(3),
                "MenuBarExtra",
                Some("/Users/test/Library/Application Support/Foo/MenuBarExtra"),
                &["MenuBarExtra"],
            ),
        ];

        let identity = resolve(&processes);
        let entity = identity
            .entities
            .get(
                identity
                    .pid_to_entity
                    .get(&4)
                    .expect("service should resolve"),
            )
            .expect("entity should exist");

        assert_eq!(entity.entity_kind, EntityKind::Service);
        assert!(entity.badges.iter().any(|badge| badge == "login-item"));
    }

    #[test]
    fn xpc_service_gets_xpc_badge() {
        let processes = vec![
            sample(
                1,
                None,
                "launchd",
                Some("/sbin/launchd"),
                &["/sbin/launchd"],
            ),
            sample(
                2,
                Some(1),
                "xpcproxy",
                Some("/usr/libexec/xpcproxy"),
                &["/usr/libexec/xpcproxy"],
            ),
            sample(
                3,
                Some(2),
                "com.apple.foo",
                Some(
                    "/System/Library/Foo.framework/XPCServices/com.apple.foo.xpc/Contents/MacOS/com.apple.foo",
                ),
                &["com.apple.foo"],
            ),
        ];

        let identity = resolve(&processes);
        let entity = identity
            .entities
            .get(
                identity
                    .pid_to_entity
                    .get(&3)
                    .expect("xpc service should resolve"),
            )
            .expect("entity should exist");

        assert!(entity.badges.iter().any(|badge| badge == "xpc-service"));
    }
}
