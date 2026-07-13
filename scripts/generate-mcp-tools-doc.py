#!/usr/bin/env python3
"""Generate docs/mcp-tools.md from the running app's live tool list.

Runs `aetower tools --json` against the bundled CLI, so the reference
documents exactly what an MCP client sees — names, descriptions, and input
schemas — rather than a hand-maintained copy that can drift. Re-run after
tool changes; the release checklist covers regenerating it.
"""

from __future__ import annotations

import json
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
CLI = ROOT / "dist" / "Aetower.app" / "Contents" / "Helpers" / "aetower"
OUT = ROOT / "docs" / "mcp-tools.md"


def schema_rows(schema: dict) -> list[str]:
    properties = schema.get("properties") or {}
    required = set(schema.get("required") or [])
    rows = []
    for name in sorted(properties):
        spec = properties[name]
        kind = spec.get("type", "any")
        if "enum" in spec:
            kind = " | ".join(f"`{v}`" for v in spec["enum"])
        notes = []
        if name in required:
            notes.append("required")
        for key in ("minimum", "maximum", "default"):
            if key in spec:
                notes.append(f"{key} {spec[key]}")
        if spec.get("description"):
            notes.append(str(spec["description"]))
        rows.append(f"| `{name}` | {kind} | {'; '.join(notes) or '—'} |")
    return rows


def main() -> None:
    if not CLI.exists():
        print(f"error: CLI not found at {CLI}", file=sys.stderr)
        raise SystemExit(1)
    raw = subprocess.run(
        [str(CLI), "tools", "--json"], capture_output=True, text=True, timeout=30
    )
    if raw.returncode != 0:
        print(f"error: aetower tools failed: {raw.stderr.strip()}", file=sys.stderr)
        raise SystemExit(1)
    payload = json.loads(raw.stdout)
    tools = payload if isinstance(payload, list) else payload["tools"]
    tools.sort(key=lambda t: t["name"])

    lines = [
        "# MCP tool reference",
        "",
        "> Generated from the running app with `aetower tools --json` "
        "(`scripts/generate-mcp-tools-doc.py`). Do not edit by hand.",
        "",
        f"Aetower's local MCP server currently exposes **{len(tools)} tools**. "
        "The server is read-only by default; guarded operator actions are a "
        "separate opt-in in Settings, and every action stays preview- and "
        "approval-gated. See [Local MCP](local-mcp.md) for the runtime model "
        "and client registration.",
        "",
        "Call any tool from the shell with "
        "`aetower call <name> [--json]`, or from any MCP client over the "
        "bundled `aetower-mcp` helper.",
        "",
    ]
    for tool in tools:
        lines.append(f"## `{tool['name']}`")
        lines.append("")
        lines.append(tool.get("description", "").strip())
        rows = schema_rows(tool.get("inputSchema") or {})
        if rows:
            lines.append("")
            lines.append("| Parameter | Type | Notes |")
            lines.append("|---|---|---|")
            lines.extend(rows)
        lines.append("")

    OUT.write_text("\n".join(lines).rstrip() + "\n")
    print(f"wrote {OUT} ({len(tools)} tools)")


if __name__ == "__main__":
    main()
