#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP_DIR="$ROOT/dist/Aetower.app"
APP_BIN="$APP_DIR/Contents/MacOS/Aetower"
MCP_BIN="$APP_DIR/Contents/Helpers/aetower-mcp"
REBUILD=0
LAUNCHED=0
APP_PID=""

while [ "$#" -gt 0 ]; do
    case "$1" in
        --rebuild)
            REBUILD=1
            shift
            ;;
        *)
            echo "unsupported argument: $1" >&2
            exit 1
            ;;
    esac
done

if [ "$REBUILD" -eq 1 ] || [ ! -x "$MCP_BIN" ]; then
    sh "$ROOT/scripts/smoke-package.sh" --rebuild
fi

EXISTING_PIDS="$(pgrep -f "$APP_BIN" || true)"
if [ -n "$EXISTING_PIDS" ]; then
    for pid in $EXISTING_PIDS; do
        kill "$pid" 2>/dev/null || true
    done
    sleep 2
fi

"$APP_BIN" >/tmp/aetower-local-operator-smoke.log 2>&1 &
APP_PID=$!
LAUNCHED=1
sleep 4

cleanup() {
    if [ "$LAUNCHED" -eq 1 ] && [ -n "$APP_PID" ]; then
        kill "$APP_PID" 2>/dev/null || true
        wait "$APP_PID" 2>/dev/null || true
    fi
}

trap cleanup EXIT INT TERM

python3 - "$MCP_BIN" <<'PY'
import json
import subprocess
import sys
import time

binary = sys.argv[1]
proc = subprocess.Popen(
    [binary],
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
    text=True,
    bufsize=1,
)


def rpc(message, timeout=45):
    assert proc.stdin is not None
    assert proc.stdout is not None
    proc.stdin.write(json.dumps(message) + "\n")
    proc.stdin.flush()
    deadline = time.time() + timeout
    while time.time() < deadline:
        line = proc.stdout.readline()
        if not line:
            break
        line = line.strip()
        if not line:
            continue
        payload = json.loads(line)
        if payload.get("id") == message.get("id"):
            return payload
    stderr = proc.stderr.read() if proc.stderr is not None else ""
    raise RuntimeError(f"no response for {message.get('method')}: {stderr}")


init = rpc(
    {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "local-operator-smoke", "version": "1.0"},
        },
    }
)
assert init["result"]["serverInfo"]["name"] == "aetower"

assert proc.stdin is not None
proc.stdin.write(
    json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized", "params": {}}) + "\n"
)
proc.stdin.flush()

tools = rpc({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}})
tool_names = {tool["name"] for tool in tools["result"]["tools"]}
for required in (
    "aetower_current_snapshot",
    "aetower_diff_snapshots",
    "aetower_entity_process_tree",
    "aetower_profile_entity",
):
    assert required in tool_names, f"missing tool {required}"

snapshot_response = rpc(
    {
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {"name": "aetower_current_snapshot", "arguments": {"entity_limit": 12}},
    }
)
snapshot = snapshot_response["result"]["structuredContent"]["snapshot"]
entities = snapshot["entities"]
assert entities, "snapshot returned no entities"
entity = next(
    (
        item
        for item in entities
        if item.get("metrics", {}).get("process_count", 0) > 0
    ),
    entities[0],
)
entity_id = entity["entity_id"]
captured_at = snapshot["captured_at_millis"]

history_summary_response = rpc(
    {
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "aetower_history_summary",
            "arguments": {"start_millis": 0, "end_millis": captured_at},
        },
    }
)
history_summary = history_summary_response["result"]["structuredContent"]
oldest_millis = history_summary["oldest_millis"]
newest_millis = history_summary["newest_millis"]
assert oldest_millis is not None and newest_millis is not None and oldest_millis != newest_millis

diff_response = rpc(
    {
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": {
            "name": "aetower_diff_snapshots",
            "arguments": {
                "before_millis": oldest_millis,
                "after_millis": newest_millis,
                "entity_ids": [entity_id],
                "limit": 4,
            },
        },
    }
)
assert "structuredContent" in diff_response["result"]

tree_response = rpc(
    {
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": {
            "name": "aetower_entity_process_tree",
            "arguments": {"entity_id": entity_id},
        },
    }
)
tree_content = tree_response["result"]["structuredContent"]
assert tree_content["root_entity_id"] == entity_id

profile_response = rpc(
    {
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": {
            "name": "aetower_profile_entity",
            "arguments": {
                "entity_id": entity_id,
                "duration_seconds": 1,
                "top_stacks": 2,
            },
        },
    },
    timeout=90,
)
assert "structuredContent" in profile_response["result"]

proc.terminate()
try:
    proc.wait(timeout=5)
except subprocess.TimeoutExpired:
    proc.kill()
    proc.wait(timeout=5)

print("✓ local operator smoke passed")
PY
