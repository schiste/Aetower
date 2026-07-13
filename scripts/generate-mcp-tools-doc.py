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



# Curated operator questions per tool — the "what does this answer" line in
# the reference and the FAQPage structured data on the published page. Keep
# in the operator's voice; a derived fallback covers unmapped new tools.
TOOL_QUESTIONS = {
    "aetower_ai_runtime_report": "What are my AI agents doing right now, and which sessions need approval?",
    "aetower_capability_status": "Which permissions and adapters are working, and what should I fix next?",
    "aetower_current_snapshot": "What is running on this Mac right now, with friction scores?",
    "aetower_diagnostics_overview": "Is Aetower itself healthy?",
    "aetower_diagnostics_summary": "What diagnostic noise keeps repeating, grouped by subsystem and severity?",
    "aetower_diff_snapshots": "What changed on this machine between two points in time?",
    "aetower_entity_details": "What is everything Aetower knows about this one app or process group?",
    "aetower_entity_group_tree": "How do processes group into apps, sessions, and agent families?",
    "aetower_entity_process_tree": "Which processes belong to this entity, and which subtree carries the burden?",
    "aetower_explain_anomalies": "Why does this app look unusual right now?",
    "aetower_export_query": "How do I export a privacy-tiered slice of Aetower's data without writing files?",
    "aetower_history_data_quality": "Are there gaps or duplicates in the recorded history window?",
    "aetower_history_page": "How do I page through stored snapshots chronologically?",
    "aetower_history_store_health": "How big is the history database, and is it healthy?",
    "aetower_history_summary": "What does the recorded history cover, at a glance?",
    "aetower_host_alerts": "Is anything on this Mac alerting right now?",
    "aetower_host_summary": "How loaded is this Mac right now — CPU, memory, energy, thermal?",
    "aetower_investigation_bundle": "My Mac froze or crashed — what happened in that window?",
    "aetower_memory_breakdown": "Where is this process's memory actually going?",
    "aetower_process_action_history": "Which process actions ran recently, and what were their outcomes?",
    "aetower_process_inspect": "What is this PID — provenance, code signing, and context?",
    "aetower_process_open_resources": "Which files, sockets, and ports does this process hold open?",
    "aetower_process_sample": "What is this process doing right now, at stack level?",
    "aetower_profile_entity": "Which threads and queues are hot in this app?",
    "aetower_query_diagnostics": "How do I search Aetower's diagnostics with filters?",
    "aetower_reboot_report": "Why did this Mac reboot, and what did it look like just before?",
    "aetower_recent_changes": "What changed recently — which processes appeared, spiked, or crashed?",
    "aetower_recommendations": "What should I do about the current pressure on this machine?",
    "aetower_repository_inventory": "Which Git repositories exist on this machine, and what state are they in?",
    "aetower_repository_scorecard": "How does this GitHub repository score on supply-chain readiness?",
    "aetower_resource_cost_rollups": "What did this repo, session, or machine cost in estimated energy, dollars, and carbon?",
    "aetower_runtime_burst_explanation": "Why did the machine just spike?",
    "aetower_runtime_lag": "Is Aetower's engine keeping up with its tick cadence?",
    "aetower_session_health": "Is the whole Aetower session healthy end to end?",
    "aetower_storage_growth_insights": "What is growing on my disk, and how fast?",
    "aetower_storage_hygiene": "How do I scan developer storage for reclaimable artifacts?",
    "aetower_storage_hygiene_actions": "What cleanup actions are available, with guardrails, without rescanning?",
    "aetower_storage_hygiene_deep_scan": "How do I run a deeper storage scan than the default?",
    "aetower_storage_hygiene_items_page": "How do I page through the ranked storage items?",
    "aetower_storage_hygiene_overview": "How much disk space can I reclaim right now, and where?",
    "aetower_storage_hygiene_repo_detail": "What is taking up space inside this one repository?",
    "aetower_support_bundle_manifest": "What would a support bundle include, before I export anything?",
    "aetower_top_findings": "What is straining my Mac right now?",
    "aetower_wakeup_attribution": "What is causing all these CPU wakeups?",
    "aetower_watch_self": "How much is Aetower itself costing the machine?",
}


def tool_question(name: str) -> str:
    fallback = "What does " + name + " return?"
    question = TOOL_QUESTIONS.get(name)
    if question is None:
        print(f"warning: no curated question for {name}; using fallback", file=sys.stderr)
        return fallback
    return question


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
    faq_entries = []
    for tool in tools:
        question = tool_question(tool["name"])
        description = tool.get("description", "").strip()
        faq_entries.append(
            {
                "q": question,
                "a": (
                    f"Use the `{tool['name']}` MCP tool "
                    f"(or `aetower call {tool['name']}` from the shell). {description}"
                ),
            }
        )
        lines.append(f"## `{tool['name']}`")
        lines.append("")
        lines.append(f"**Answers:** \u201c{question}\u201d")
        lines.append("")
        lines.append(description)
        rows = schema_rows(tool.get("inputSchema") or {})
        if rows:
            lines.append("")
            lines.append("| Parameter | Type | Notes |")
            lines.append("|---|---|---|")
            lines.extend(rows)
        lines.append("")

    OUT.write_text("\n".join(lines).rstrip() + "\n")
    sidecar = OUT.with_suffix(".faq.json")
    sidecar.write_text(json.dumps(faq_entries, indent=1, ensure_ascii=False) + "\n")
    print(f"wrote {OUT} ({len(tools)} tools) and {sidecar.name}")


if __name__ == "__main__":
    main()
