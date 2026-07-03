//! Thin client over a running Aetower's local MCP socket.
//!
//! Every verb ultimately calls [`Client::fetch`], which resolves the socket
//! path, verifies the app is actually listening (so we can print a friendly
//! "start Aetower" message instead of a raw connection error), and returns the
//! tool payload as JSON. The heavy lifting — framing, `tools/call`, and
//! `structuredContent` extraction — lives in `aetower_mcp::call_tool`, so this
//! CLI stays a genuinely thin second client of the same engine the app and the
//! agents talk to.

use std::path::{Path, PathBuf};

use serde_json::Value;

/// Exit codes. Kept small and stable so scripts can branch on them.
pub mod exit {
    /// Everything succeeded.
    pub const OK: i32 = 0;
    /// The app isn't running / the socket isn't reachable.
    pub const UNREACHABLE: i32 = 3;
    /// A tool or protocol error came back from the app.
    pub const TOOL_ERROR: i32 = 4;
    /// A `--fail-on-warn` gate tripped (alerts/budgets breached).
    pub const ALERT: i32 = 5;
    /// A local action (e.g. install) failed.
    pub const LOCAL: i32 = 6;
}

/// A resolved connection to one Aetower socket.
pub struct Client {
    socket_path: PathBuf,
}

impl Client {
    /// Resolve the socket: an explicit override, else the app's default
    /// (`~/.aetower/mcp.sock`).
    pub fn new(socket_override: Option<PathBuf>) -> Self {
        let socket_path = socket_override.unwrap_or_else(aetower_mcp::default_socket_path);
        Self { socket_path }
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// True when a live Aetower is listening on the socket.
    pub fn is_reachable(&self) -> bool {
        aetower_mcp::is_socket_listener_reachable(&self.socket_path)
    }

    /// One-line message for the not-running case, shared by every verb.
    pub fn unreachable_message(&self) -> String {
        format!(
            "Aetower isn't running (no MCP socket at {}).\n\
             Launch the app first:  open -a Aetower",
            self.socket_path.display()
        )
    }

    /// Call a tool and return its JSON payload, or an error string.
    pub fn fetch(&self, tool: &str, arguments: Value) -> Result<Value, String> {
        aetower_mcp::call_tool(&self.socket_path, tool, arguments)
    }

    /// Fetch the tool list the app advertises (`{ "tools": [ … ] }`).
    pub fn tools(&self) -> Result<Value, String> {
        aetower_mcp::list_tools(&self.socket_path)
    }
}
