use std::{
    collections::{BTreeMap, BTreeSet},
    io::{BufRead, BufReader, ErrorKind, Write},
    os::unix::net::UnixStream,
    time::Duration,
};

use serde::Deserialize;
use serde_json::{Value, json};

const SOCKET_TIMEOUT: Duration = Duration::from_millis(1_500);
const MAX_RPC_LINES: usize = 24;

/// A tab from Chau7's `tab_list` MCP tool response.
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct Chau7Tab {
    pub tab_id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub cwd: String,
    pub repo_root: Option<String>,
    pub git_branch: Option<String>,
    pub ai_provider: Option<String>,
    pub ai_session_id: Option<String>,
    #[serde(default)]
    pub status: String,
    pub active_app: Option<String>,
    #[serde(default)]
    pub window_id: u32,
}

/// A session from Chau7's `session_list` MCP tool response.
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct Chau7Session {
    pub session_id: String,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub repo_path: String,
    #[serde(default)]
    pub run_count: u32,
    #[serde(default)]
    pub last_active: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Chau7PendingApproval {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub description: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Chau7ActiveRun {
    #[serde(default, rename = "duration_so_far_ms")]
    pub duration_so_far_ms: u64,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub run_id: String,
    #[serde(default)]
    pub session_id: String,
    #[serde(default, rename = "started_at")]
    pub started_at: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Chau7TabStatus {
    #[serde(default)]
    pub tab_id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub cwd: String,
    pub repo_root: Option<String>,
    pub git_branch: Option<String>,
    pub ai_provider: Option<String>,
    pub ai_session_id: Option<String>,
    #[serde(default)]
    pub status: String,
    pub active_app: Option<String>,
    #[serde(default)]
    pub is_at_prompt: bool,
    #[serde(default)]
    pub shell_loading: bool,
    #[serde(default)]
    pub cto_active: bool,
    pub raw_status: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Chau7RuntimeSessionStatus {
    #[serde(default)]
    pub session_id: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub turn_count: u32,
    #[serde(default)]
    pub last_completed_turn_id: Option<String>,
    #[serde(default)]
    pub last_exit_reason: Option<String>,
    #[serde(default)]
    pub pending_approval: Option<Chau7PendingApproval>,
    #[serde(default)]
    pub active_run: Option<Chau7ActiveRun>,
    #[serde(default)]
    pub child_session_count: u32,
}

/// Per-repo cost stats from Chau7's `repo_get_metadata` tool.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Chau7RepoStats {
    #[serde(default)]
    pub total_runs: u32,
    #[serde(default)]
    pub total_tokens: u64,
    #[serde(default)]
    pub total_cost: f32,
    #[serde(default)]
    pub total_turns: u32,
    #[serde(default)]
    pub providers: Vec<String>,
}

/// A run from Chau7's `run_list` tool response.
#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
pub struct Chau7Run {
    pub id: String,
    #[serde(default, rename = "startedAt")]
    pub started_at: String,
    #[serde(default, rename = "endedAt")]
    pub ended_at: Option<String>,
    #[serde(default, rename = "sessionID")]
    pub session_id: Option<String>,
    #[serde(default)]
    pub provider: String,
    #[serde(default, rename = "durationMs")]
    pub duration_ms: Option<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Chau7RepoEvent {
    #[serde(default, rename = "type")]
    pub event_type: String,
    #[serde(default, rename = "ts")]
    pub timestamp: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub tab_id: Option<String>,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub tool: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Chau7RuntimeInfo {
    #[serde(default)]
    pub app_version: Option<String>,
    #[serde(default)]
    pub build_sha: Option<String>,
    #[serde(default)]
    pub build_timestamp: Option<String>,
    #[serde(default)]
    pub build_channel: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Deserialize)]
struct Chau7SessionChild {
    #[serde(default)]
    pub session_id: String,
}

/// Combined snapshot cached by the adapter on each refresh cycle.
#[derive(Debug, Clone, Default)]
pub struct Chau7Snapshot {
    pub tabs: Vec<Chau7Tab>,
    pub sessions: Vec<Chau7Session>,
    pub runtime_info: Option<Chau7RuntimeInfo>,
    pub repo_stats: BTreeMap<String, Chau7RepoStats>,
    pub recent_runs: Vec<Chau7Run>,
    pub tab_statuses: BTreeMap<String, Chau7TabStatus>,
    pub runtime_sessions: BTreeMap<String, Chau7RuntimeSessionStatus>,
    pub repo_events: BTreeMap<String, Vec<Chau7RepoEvent>>,
}

#[derive(Debug, Clone, Copy)]
pub struct Chau7FetchOptions {
    pub max_ai_tabs: usize,
    pub max_repos: usize,
    pub include_deep_context: bool,
}

impl Default for Chau7FetchOptions {
    fn default() -> Self {
        Self {
            max_ai_tabs: 8,
            max_repos: 3,
            include_deep_context: true,
        }
    }
}

impl Chau7Tab {
    /// Whether this tab is running an AI agent (has a recognized provider).
    pub fn is_ai_agent(&self) -> bool {
        self.ai_provider.is_some()
    }

    /// Normalised provider label for badges (lowercase).
    pub fn provider_label(&self) -> Option<&str> {
        self.ai_provider.as_deref()
    }
}

impl Chau7Snapshot {
    pub fn inherit_deep_context_from(&mut self, previous: &Self) {
        if self.runtime_info.is_none() {
            self.runtime_info = previous.runtime_info.clone();
        }
        let active_tab_ids = self
            .tabs
            .iter()
            .map(|tab| tab.tab_id.clone())
            .collect::<BTreeSet<_>>();
        for (tab_id, status) in &previous.tab_statuses {
            if active_tab_ids.contains(tab_id) && !self.tab_statuses.contains_key(tab_id) {
                self.tab_statuses.insert(tab_id.clone(), status.clone());
            }
        }

        let active_session_ids = self
            .tabs
            .iter()
            .filter_map(|tab| tab.ai_session_id.clone())
            .chain(
                self.sessions
                    .iter()
                    .map(|session| session.session_id.clone()),
            )
            .collect::<BTreeSet<_>>();
        for (session_id, session) in &previous.runtime_sessions {
            if active_session_ids.contains(session_id)
                && !self.runtime_sessions.contains_key(session_id)
            {
                self.runtime_sessions
                    .insert(session_id.clone(), session.clone());
            }
        }

        let active_repos = self
            .tabs
            .iter()
            .filter_map(|tab| tab.repo_root.clone())
            .collect::<BTreeSet<_>>();
        for repo in &active_repos {
            if !self.repo_stats.contains_key(repo)
                && let Some(stats) = previous.repo_stats.get(repo)
            {
                self.repo_stats.insert(repo.clone(), stats.clone());
            }
            if !self.repo_events.contains_key(repo)
                && let Some(events) = previous.repo_events.get(repo)
            {
                self.repo_events.insert(repo.clone(), events.clone());
            }
        }
        if self.recent_runs.is_empty() {
            self.recent_runs = previous.recent_runs.clone();
        }
    }
}

// ---------------------------------------------------------------------------
// MCP JSON-RPC 2.0 client (line-delimited over Unix socket)
// ---------------------------------------------------------------------------

/// Connect to Chau7's MCP socket and fetch a snapshot with explicit bounds for
/// expensive adapter calls.
///
/// The adapter uses this to alternate cheap, frequent identity refreshes with
/// less frequent deep context pulls. That keeps Chau7 context useful without
/// forcing Chau7 to recompute repo/runtime detail on every Aetower tick.
pub fn fetch_snapshot_with_options(
    socket_path: &str,
    options: Chau7FetchOptions,
) -> Result<Chau7Snapshot, String> {
    let stream =
        UnixStream::connect(socket_path).map_err(|e| format!("connect {socket_path}: {e}"))?;
    stream
        .set_read_timeout(Some(SOCKET_TIMEOUT))
        .map_err(|e| format!("set_read_timeout: {e}"))?;
    stream
        .set_write_timeout(Some(SOCKET_TIMEOUT))
        .map_err(|e| format!("set_write_timeout: {e}"))?;

    let mut reader = BufReader::new(stream.try_clone().map_err(|e| format!("clone: {e}"))?);
    let mut writer = stream;

    // MCP handshake: initialize + initialized notification.
    let _init_result = rpc_call(
        &mut writer,
        &mut reader,
        1,
        "initialize",
        json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "aetower", "version": "0.1.0" }
        }),
    )?;

    send_notification(&mut writer, "notifications/initialized", json!({}))?;

    // Fetch tab list.
    let tabs_raw = rpc_tool_call(&mut writer, &mut reader, 2, "tab_list", json!({}))?;
    let tabs: Vec<Chau7Tab> =
        serde_json::from_value(tabs_raw).map_err(|e| format!("parse tabs: {e}"))?;

    // Fetch session list.
    let sessions_raw = rpc_tool_call(&mut writer, &mut reader, 3, "session_list", json!({}))?;
    let sessions: Vec<Chau7Session> =
        serde_json::from_value(sessions_raw).map_err(|e| format!("parse sessions: {e}"))?;

    let mut next_id: u64 = 4;
    let mut runtime_info = None;
    for tool_name in ["chau7_runtime_info", "runtime_info"] {
        let parsed = rpc_tool_call(&mut writer, &mut reader, next_id, tool_name, json!({}))
            .ok()
            .and_then(|raw| serde_json::from_value::<Chau7RuntimeInfo>(raw).ok());
        next_id += 1;
        if parsed.is_some() {
            runtime_info = parsed;
            break;
        }
    }

    // Fetch deeper runtime/tab state for AI tabs (best-effort, bounded).
    let mut tab_statuses = BTreeMap::new();
    let mut runtime_sessions = BTreeMap::new();
    let mut seen_sessions = BTreeSet::new();
    for tab in tabs
        .iter()
        .filter(|t| t.is_ai_agent())
        .take(options.max_ai_tabs)
    {
        if let Ok(raw) = rpc_tool_call(
            &mut writer,
            &mut reader,
            next_id,
            "tab_status",
            json!({ "tab_id": &tab.tab_id }),
        ) && let Ok(status) = serde_json::from_value::<Chau7TabStatus>(raw)
        {
            tab_statuses.insert(tab.tab_id.clone(), status);
        }
        next_id += 1;

        let Some(session_id) = tab.ai_session_id.as_deref() else {
            continue;
        };
        if !seen_sessions.insert(session_id.to_owned()) {
            continue;
        }

        let mut merged = rpc_tool_call(
            &mut writer,
            &mut reader,
            next_id,
            "runtime_session_get",
            json!({ "session_id": session_id }),
        )
        .ok()
        .and_then(|raw| serde_json::from_value::<Chau7RuntimeSessionStatus>(raw).ok())
        .unwrap_or_else(|| Chau7RuntimeSessionStatus {
            session_id: session_id.to_owned(),
            ..Chau7RuntimeSessionStatus::default()
        });
        next_id += 1;

        if options.include_deep_context {
            if let Ok(raw) = rpc_tool_call(
                &mut writer,
                &mut reader,
                next_id,
                "runtime_turn_status",
                json!({ "session_id": session_id }),
            ) && let Ok(turn_status) = serde_json::from_value::<Chau7RuntimeSessionStatus>(raw)
            {
                if merged.state.is_empty() {
                    merged.state = turn_status.state;
                }
                if merged.pending_approval.is_none() {
                    merged.pending_approval = turn_status.pending_approval;
                }
                if merged.active_run.is_none() {
                    merged.active_run = turn_status.active_run;
                }
                if merged.last_exit_reason.is_none() {
                    merged.last_exit_reason = turn_status.last_exit_reason;
                }
                if merged.turn_count == 0 {
                    merged.turn_count = turn_status.turn_count;
                }
                if merged.last_completed_turn_id.is_none() {
                    merged.last_completed_turn_id = turn_status.last_completed_turn_id;
                }
            }
            next_id += 1;
        }

        // Skip runtime_session_children: it forces Chau7 to do a recursive
        // tree walk over a JSON-RPC boundary on every adapter tick, and was
        // identified as the single biggest contributor to observer-induced
        // load. child_session_count stays at 0 for now; if we need it back,
        // expose it as a counter on Chau7's side instead of asking the
        // server to recompute it on every poll.

        runtime_sessions.insert(session_id.to_owned(), merged);
    }

    // Fetch repo stats for active repos (best-effort, bounded by options).
    let mut repo_stats = BTreeMap::new();
    let mut repo_events = BTreeMap::new();
    let mut seen_repos = BTreeSet::new();
    if options.include_deep_context && options.max_repos > 0 {
        for tab in tabs.iter().filter(|t| t.is_ai_agent()) {
            if let Some(repo) = tab.repo_root.as_deref() {
                if !seen_repos.insert(repo.to_owned()) || seen_repos.len() > options.max_repos {
                    continue;
                }
                if let Ok(raw) = rpc_tool_call(
                    &mut writer,
                    &mut reader,
                    next_id,
                    "repo_get_metadata",
                    json!({ "repo_path": repo }),
                ) && let Some(stats) = raw.get("stats")
                    && let Ok(parsed) = serde_json::from_value::<Chau7RepoStats>(stats.clone())
                {
                    repo_stats.insert(repo.to_owned(), parsed);
                }
                next_id += 1;

                if let Ok(raw) = rpc_tool_call(
                    &mut writer,
                    &mut reader,
                    next_id,
                    "repo_get_events",
                    json!({ "repo_path": repo, "limit": 12 }),
                ) && let Some(events) = raw.get("events")
                    && let Ok(parsed) =
                        serde_json::from_value::<Vec<Chau7RepoEvent>>(events.clone())
                {
                    repo_events.insert(repo.to_owned(), parsed);
                }
                next_id += 1;
            }
        }
    }

    // Fetch recent runs for session markers (best-effort, limit 10).
    let recent_runs = if options.include_deep_context {
        rpc_tool_call(
            &mut writer,
            &mut reader,
            next_id,
            "run_list",
            json!({ "limit": 10 }),
        )
        .ok()
        .and_then(|v| serde_json::from_value::<Vec<Chau7Run>>(v).ok())
        .unwrap_or_default()
    } else {
        Vec::new()
    };

    Ok(Chau7Snapshot {
        tabs,
        sessions,
        runtime_info,
        repo_stats,
        recent_runs,
        tab_statuses,
        runtime_sessions,
        repo_events,
    })
}

/// Stop a Chau7 runtime session via the `runtime_session_stop` MCP tool.
pub fn stop_session(socket_path: &str, session_id: &str, force: bool) -> Result<(), String> {
    let stream =
        UnixStream::connect(socket_path).map_err(|e| format!("connect {socket_path}: {e}"))?;
    stream
        .set_read_timeout(Some(SOCKET_TIMEOUT))
        .map_err(|e| format!("set_read_timeout: {e}"))?;
    stream
        .set_write_timeout(Some(SOCKET_TIMEOUT))
        .map_err(|e| format!("set_write_timeout: {e}"))?;

    let mut reader = BufReader::new(stream.try_clone().map_err(|e| format!("clone: {e}"))?);
    let mut writer = stream;

    let _init = rpc_call(
        &mut writer,
        &mut reader,
        1,
        "initialize",
        json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "aetower", "version": "0.1.0" }
        }),
    )?;

    send_notification(&mut writer, "notifications/initialized", json!({}))?;

    let _result = rpc_tool_call(
        &mut writer,
        &mut reader,
        2,
        "runtime_session_stop",
        json!({ "session_id": session_id, "force": force }),
    )?;

    Ok(())
}

// ---------------------------------------------------------------------------
// JSON-RPC helpers
// ---------------------------------------------------------------------------

/// Send a `tools/call` request and unwrap the double-wrapped MCP content
/// envelope: `result.content[0].text` → parsed JSON value.
fn rpc_tool_call(
    writer: &mut UnixStream,
    reader: &mut BufReader<UnixStream>,
    id: u64,
    tool_name: &str,
    arguments: Value,
) -> Result<Value, String> {
    let result = rpc_call(
        writer,
        reader,
        id,
        "tools/call",
        json!({ "name": tool_name, "arguments": arguments }),
    )?;

    // MCP wraps tool output in {"content": [{"type":"text","text":"..."}]}.
    let text = result
        .get("content")
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.first())
        .and_then(|item| item.get("text"))
        .and_then(|t| t.as_str())
        .ok_or_else(|| format!("unexpected tool response shape for {tool_name}"))?;

    serde_json::from_str(text).map_err(|e| format!("parse tool text for {tool_name}: {e}"))
}

/// Low-level JSON-RPC 2.0 request → response over a line-delimited stream.
fn rpc_call(
    writer: &mut UnixStream,
    reader: &mut BufReader<UnixStream>,
    id: u64,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    let request = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    });

    let mut line = serde_json::to_string(&request).map_err(|e| format!("serialize: {e}"))?;
    line.push('\n');
    writer
        .write_all(line.as_bytes())
        .map_err(|e| format!("write {method}: {e}"))?;
    writer.flush().map_err(|e| format!("flush {method}: {e}"))?;

    // Read lines until we get a response with a matching id.
    // MCP servers may send notifications (no "id" field) at any time;
    // we skip those to avoid mistaking them for the response.
    for _ in 0..MAX_RPC_LINES {
        let mut response_line = String::new();
        match reader.read_line(&mut response_line) {
            Ok(_) => {}
            Err(error) if is_transient_read_error(&error) => {
                continue;
            }
            Err(error) => return Err(format!("read {method}: {error}")),
        }

        if response_line.trim().is_empty() {
            continue;
        }

        let response: Value =
            serde_json::from_str(&response_line).map_err(|e| format!("parse response: {e}"))?;

        // Skip server-initiated notifications (no "id" field).
        if response.get("id").is_none() || response.get("id") == Some(&Value::Null) {
            continue;
        }

        if let Some(err) = response.get("error") {
            return Err(format!(
                "rpc error for {method}: {}",
                serde_json::to_string(err).unwrap_or_default()
            ));
        }

        return response
            .get("result")
            .cloned()
            .ok_or_else(|| format!("missing result for {method}"));
    }

    Err(format!(
        "no response for {method} after {MAX_RPC_LINES} lines (all were notifications or transient timeouts)"
    ))
}

fn is_transient_read_error(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
    ) || error.raw_os_error() == Some(35)
}

/// Send a JSON-RPC notification (no id, no response expected).
fn send_notification(writer: &mut UnixStream, method: &str, params: Value) -> Result<(), String> {
    let notification = json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
    });

    let mut line =
        serde_json::to_string(&notification).map_err(|e| format!("serialize notif: {e}"))?;
    line.push('\n');
    writer
        .write_all(line.as_bytes())
        .map_err(|e| format!("write notif {method}: {e}"))?;
    writer
        .flush()
        .map_err(|e| format!("flush notif {method}: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tab_list_response() {
        let raw = r#"[
            {
                "tab_id": "38D362ED-9295-4DF9-9A31-381CBB1BE88A",
                "title": "Bug report",
                "cwd": "/Users/dev/Chau7",
                "repo_root": "/Users/dev/Chau7",
                "git_branch": "main",
                "ai_provider": "claude",
                "ai_session_id": "8faa58a3-6fad-4f78-8ae6-a6d48c0c5314",
                "status": "idle",
                "active_app": "Claude",
                "window_id": 0
            },
            {
                "tab_id": "DF550B81-3D58-4DD6-B382-B0C6B12BC907",
                "title": "Shell",
                "cwd": "/Users/dev/Mockup",
                "status": "idle",
                "window_id": 0
            }
        ]"#;

        let tabs: Vec<Chau7Tab> = serde_json::from_str(raw).unwrap();
        assert_eq!(tabs.len(), 2);
        assert!(tabs[0].is_ai_agent());
        assert_eq!(tabs[0].provider_label(), Some("claude"));
        assert!(!tabs[1].is_ai_agent());
    }

    #[test]
    fn parse_session_list_response() {
        let raw = r#"[
            {
                "session_id": "019d4e75-2caa-7732-8c03-be5574771f82",
                "provider": "codex",
                "repo_path": "/Users/dev/Aetower",
                "run_count": 3,
                "last_active": "2026-04-02T15:59:57.303Z"
            }
        ]"#;

        let sessions: Vec<Chau7Session> = serde_json::from_str(raw).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].provider, "codex");
        assert_eq!(sessions[0].run_count, 3);
    }

    #[test]
    fn parse_runtime_session_status() {
        let raw = r#"{
            "session_id": "rs_36e6122b",
            "state": "ready",
            "turn_count": 3,
            "last_completed_turn_id": "t_3",
            "last_exit_reason": "error",
            "pending_approval": {
                "id": "22CEC148-B1A9-45AC-A67F-059710B0F85E",
                "description": "Claude needs your permission to use Bash"
            },
            "active_run": {
                "duration_so_far_ms": 438172,
                "provider": "claude",
                "run_id": "C01DBDCB-E032-4E4F-A0BE-69D99CF93197",
                "session_id": "00cc9d06-4fcf-48df-b3a8-c6a53a2788f8",
                "started_at": "2026-04-08T14:22:26.696Z"
            }
        }"#;

        let status: Chau7RuntimeSessionStatus = serde_json::from_str(raw).unwrap();
        assert_eq!(status.state, "ready");
        assert_eq!(status.turn_count, 3);
        assert_eq!(status.last_exit_reason.as_deref(), Some("error"));
        assert_eq!(
            status
                .pending_approval
                .as_ref()
                .map(|value| value.description.as_str()),
            Some("Claude needs your permission to use Bash")
        );
        assert_eq!(
            status
                .active_run
                .as_ref()
                .map(|value| value.duration_so_far_ms),
            Some(438172)
        );
    }

    #[test]
    fn parse_repo_events_response() {
        let raw = r#"[
            {
                "directory": "/Users/dev/Aetower",
                "id": "0EC21096-48E9-450F-A708-945A19FE1B83",
                "message": "Claude needs your permission to use Write",
                "session_id": "00cc9d06-4fcf-48df-b3a8-c6a53a2788f8",
                "source": "claude_code",
                "tab_id": "3D4924F3-24CA-4D4B-8707-2EB7E29C1EB9",
                "tool": "Claude",
                "ts": "2026-04-08T13:43:55.170Z",
                "type": "permission"
            }
        ]"#;

        let events: Vec<Chau7RepoEvent> = serde_json::from_str(raw).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "permission");
        assert_eq!(events[0].source, "claude_code");
    }

    #[test]
    fn parse_runtime_info_response() {
        let raw = r#"{
            "app_version": "1.4.2",
            "build_sha": "abc123def456",
            "build_timestamp": "2026-04-14T11:41:49Z",
            "build_channel": "dev"
        }"#;

        let runtime_info: Chau7RuntimeInfo = match serde_json::from_str(raw) {
            Ok(runtime_info) => runtime_info,
            Err(error) => panic!("parse runtime info: {error}"),
        };
        assert_eq!(runtime_info.app_version.as_deref(), Some("1.4.2"));
        assert_eq!(runtime_info.build_sha.as_deref(), Some("abc123def456"));
        assert_eq!(
            runtime_info.build_timestamp.as_deref(),
            Some("2026-04-14T11:41:49Z")
        );
        assert_eq!(runtime_info.build_channel.as_deref(), Some("dev"));
    }

    #[test]
    fn light_snapshot_inherits_deep_context_for_active_items() {
        let mut previous = Chau7Snapshot {
            runtime_info: Some(Chau7RuntimeInfo {
                app_version: Some("1.4.2".to_owned()),
                build_sha: Some("abc123".to_owned()),
                build_timestamp: None,
                build_channel: Some("dev".to_owned()),
            }),
            recent_runs: vec![Chau7Run {
                id: "run-1".to_owned(),
                started_at: "2026-04-20T08:00:00Z".to_owned(),
                ended_at: None,
                session_id: Some("sess-1".to_owned()),
                provider: "claude".to_owned(),
                duration_ms: Some(42),
            }],
            ..Chau7Snapshot::default()
        };
        previous.tab_statuses.insert(
            "tab-1".to_owned(),
            Chau7TabStatus {
                tab_id: "tab-1".to_owned(),
                status: "ready".to_owned(),
                ..Chau7TabStatus::default()
            },
        );
        previous.runtime_sessions.insert(
            "sess-1".to_owned(),
            Chau7RuntimeSessionStatus {
                session_id: "sess-1".to_owned(),
                state: "ready".to_owned(),
                turn_count: 7,
                ..Chau7RuntimeSessionStatus::default()
            },
        );
        previous.repo_stats.insert(
            "/repo".to_owned(),
            Chau7RepoStats {
                total_runs: 3,
                total_tokens: 1_234,
                total_cost: 0.42,
                total_turns: 9,
                providers: vec!["claude".to_owned()],
            },
        );
        previous.repo_stats.insert(
            "/stale".to_owned(),
            Chau7RepoStats {
                total_runs: 99,
                ..Chau7RepoStats::default()
            },
        );
        previous.repo_events.insert(
            "/repo".to_owned(),
            vec![Chau7RepoEvent {
                event_type: "permission".to_owned(),
                message: "approval required".to_owned(),
                ..Chau7RepoEvent::default()
            }],
        );

        let mut light = Chau7Snapshot {
            tabs: vec![Chau7Tab {
                tab_id: "tab-1".to_owned(),
                title: "Agent".to_owned(),
                cwd: "/repo".to_owned(),
                repo_root: Some("/repo".to_owned()),
                git_branch: Some("main".to_owned()),
                ai_provider: Some("claude".to_owned()),
                ai_session_id: Some("sess-1".to_owned()),
                status: "running".to_owned(),
                active_app: Some("Claude".to_owned()),
                window_id: 1,
            }],
            ..Chau7Snapshot::default()
        };

        light.inherit_deep_context_from(&previous);

        assert_eq!(
            light
                .runtime_info
                .as_ref()
                .and_then(|info| info.app_version.as_deref()),
            Some("1.4.2")
        );
        assert_eq!(
            light
                .tab_statuses
                .get("tab-1")
                .map(|status| status.status.as_str()),
            Some("ready")
        );
        assert_eq!(
            light
                .runtime_sessions
                .get("sess-1")
                .map(|session| session.turn_count),
            Some(7)
        );
        assert_eq!(
            light
                .repo_stats
                .get("/repo")
                .map(|stats| stats.total_tokens),
            Some(1_234)
        );
        assert!(!light.repo_stats.contains_key("/stale"));
        assert_eq!(light.repo_events.get("/repo").map(Vec::len), Some(1));
        assert_eq!(light.recent_runs.len(), 1);
    }
}
