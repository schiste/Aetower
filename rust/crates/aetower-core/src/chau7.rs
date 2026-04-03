use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    time::Duration,
};

use serde::Deserialize;
use serde_json::{Value, json};

const SOCKET_TIMEOUT: Duration = Duration::from_millis(500);

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

/// Per-repo cost stats from Chau7's `repo_get_metadata` tool.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Chau7RepoStats {
    #[serde(default)]
    pub total_runs: u32,
    #[serde(default)]
    pub total_tokens: u64,
    #[serde(default)]
    pub total_cost: f32,
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

/// Combined snapshot cached by the adapter on each refresh cycle.
#[derive(Debug, Clone, Default)]
pub struct Chau7Snapshot {
    pub tabs: Vec<Chau7Tab>,
    pub sessions: Vec<Chau7Session>,
    pub repo_stats: BTreeMap<String, Chau7RepoStats>,
    pub recent_runs: Vec<Chau7Run>,
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

// ---------------------------------------------------------------------------
// MCP JSON-RPC 2.0 client (line-delimited over Unix socket)
// ---------------------------------------------------------------------------

/// Connect to Chau7's MCP socket, run the initialize handshake, call
/// `tab_list` and `session_list`, then return the combined snapshot.
pub fn fetch_snapshot(socket_path: &str) -> Result<Chau7Snapshot, String> {
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

    // Fetch repo stats for active repos (best-effort, limit 3 repos).
    let mut repo_stats = BTreeMap::new();
    let mut seen_repos = std::collections::BTreeSet::new();
    let mut next_id: u64 = 4;
    for tab in tabs.iter().filter(|t| t.is_ai_agent()) {
        if let Some(repo) = tab.repo_root.as_deref() {
            if !seen_repos.insert(repo.to_owned()) || seen_repos.len() > 3 {
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
        }
    }

    // Fetch recent runs for session markers (best-effort, limit 10).
    let recent_runs = rpc_tool_call(
        &mut writer,
        &mut reader,
        next_id,
        "run_list",
        json!({ "limit": 10 }),
    )
    .ok()
    .and_then(|v| serde_json::from_value::<Vec<Chau7Run>>(v).ok())
    .unwrap_or_default();

    Ok(Chau7Snapshot {
        tabs,
        sessions,
        repo_stats,
        recent_runs,
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
    let max_lines = 16;
    for _ in 0..max_lines {
        let mut response_line = String::new();
        reader
            .read_line(&mut response_line)
            .map_err(|e| format!("read {method}: {e}"))?;

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
        "no response for {method} after {max_lines} lines (all were notifications)"
    ))
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
}
