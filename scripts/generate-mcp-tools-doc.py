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
            kind = " · ".join(f"`{v}`" for v in spec["enum"])
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



# Tools whose example call would be heavy or destructive to run casually.
EXAMPLE_SKIP = {
    "aetower_storage_hygiene_deep_scan",
    "aetower_storage_hygiene",
}

# Optional args that keep example outputs small.
OPTIONAL_EXAMPLE_ARGS = {
    "aetower_current_snapshot": {"entity_limit": 2},
    "aetower_query_diagnostics": {"limit": 3},
    "aetower_recent_changes": {"limit": 3},
}

HOME = str(pathlib.Path.home())
USERNAME = pathlib.Path.home().name
EXAMPLES_CACHE = ROOT / "docs" / "mcp-tools.examples.json"


def load_examples_cache() -> dict:
    if EXAMPLES_CACHE.exists():
        return json.loads(EXAMPLES_CACHE.read_text())
    return {}


def save_examples_cache(cache: dict) -> None:
    EXAMPLES_CACHE.write_text(json.dumps(cache, indent=1, ensure_ascii=False) + "\n")


def call_tool(name: str, args: dict, attempts: int = 3) -> dict | list | None:
    cmd = [str(CLI), "call", name, "--json"]
    for key, value in args.items():
        cmd += ["--arg", f"{key}={json.dumps(value)}"]
    for attempt in range(attempts):
        if attempt:
            __import__("time").sleep(8)  # let a busy engine catch its breath
        try:
            raw = subprocess.run(cmd, capture_output=True, text=True, timeout=90)
        except subprocess.TimeoutExpired:
            continue
        if raw.returncode != 0:
            continue
        try:
            return json.loads(raw.stdout)
        except json.JSONDecodeError:
            continue
    return None


def seed_values() -> dict:
    seeds = {}
    now = int(__import__("time").time() * 1000)
    seeds["end_millis"] = now
    seeds["start_millis"] = now - 30 * 60 * 1000
    seeds["after_millis"] = now
    seeds["before_millis"] = now - 30 * 60 * 1000
    payload = call_tool("aetower_current_snapshot", {"entity_limit": 3}) or {}
    snapshot = payload.get("snapshot") or payload
    entities = snapshot.get("entities") or []
    if entities:
        seeds["entity_id"] = entities[0].get("entity_id") or entities[0].get("id")
        for entity in entities:
            for component in entity.get("components") or []:
                pid = (
                    component.get("process_id") or component.get("pid")
                ) if isinstance(component, dict) else None
                if pid:
                    seeds["pid"] = pid
                    break
            if "pid" in seeds:
                break
    inventory = call_tool("aetower_repository_inventory", {}) or {}
    repos = inventory.get("repository_inventory") or inventory.get("repositories") or []
    for repo in repos if isinstance(repos, list) else []:
        root = repo.get("root") or repo.get("path") or repo.get("repo_root")
        if root:
            seeds["repo_root"] = root
            break
    missing = {"entity_id", "pid"} - set(seeds)
    if missing:
        print(f"warning: seeds missing after snapshot: {sorted(missing)}", file=sys.stderr)
    return {k: v for k, v in seeds.items() if v is not None}


def sanitize(value, depth: int = 0):
    if depth >= 5 and isinstance(value, (dict, list)):
        return "…"
    if isinstance(value, dict):
        items = list(value.items())
        trimmed = {k: sanitize(v, depth + 1) for k, v in items[:12]}
        if len(items) > 12:
            trimmed["…"] = f"{len(items) - 12} more fields"
        return trimmed
    if isinstance(value, list):
        if len(value) > 2:
            return [sanitize(value[0], depth + 1), f"… {len(value) - 1} more items"]
        return [sanitize(v, depth + 1) for v in value]
    if isinstance(value, str):
        text = value.replace(HOME, "~").replace(USERNAME, "operator")
        return text[:96] + "…" if len(text) > 96 else text
    return value


def example_block(name: str, schema: dict, seeds: dict, cache: dict) -> list[str]:
    if name in EXAMPLE_SKIP:
        return []
    cached = cache.get(name)
    if cached is None:
        required = schema.get("required") or []
        args = {}
        for key in required:
            if key not in seeds:
                print(f"warning: no seed for {name}.{key}; skipping example", file=sys.stderr)
                return []
            args[key] = seeds[key]
        args.update(OPTIONAL_EXAMPLE_ARGS.get(name, {}))
        payload = call_tool(name, args, attempts=1)
        if payload is None:
            print(f"warning: example call failed for {name} (will retry next run)", file=sys.stderr)
            return []
        # Cache the SANITIZED payload: times/paths are already normalized and
        # nothing personal is persisted. Time-window args are cached as a
        # readable placeholder so the invocation line stays meaningful.
        display_args = {
            k: ("<epoch-millis>" if k.endswith("_millis") else v) for k, v in args.items()
        }
        cached = {"args": display_args, "output": sanitize(payload)}
        cache[name] = cached
        save_examples_cache(cache)
    rendered = json.dumps(cached["output"], indent=1, ensure_ascii=False)
    lines = rendered.splitlines()
    if len(lines) > 40:
        lines = lines[:40] + ["…"]
    shown_args = " ".join(
        f"--arg {k}={v if v == '<epoch-millis>' else json.dumps(v)}"
        for k, v in cached["args"].items()
    )
    invocation = f"aetower call {name}" + (f" {shown_args}" if shown_args else "")
    return [
        "",
        f"Example — `{invocation}`:",
        "",
        "```json",
        *lines,
        "```",
    ]

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
    cache = load_examples_cache()
    missing = [t["name"] for t in tools if t["name"] not in cache and t["name"] not in EXAMPLE_SKIP]
    seeds = {}
    if missing:
        print(f"collecting example seeds… ({len(missing)} examples missing)", file=sys.stderr)
        seeds = seed_values()

    lines = [
        "# MCP tool reference",
        "",
        "> Generated from the running app with `aetower tools --json` "
        "(`scripts/generate-mcp-tools-doc.py`). Do not edit by hand.",
        "",
        f"Aetower's local MCP server currently exposes **{len(tools)} tools**. "
        "Guarded operator actions are visible to trusted local clients by "
        "default, every action stays preview- and approval-gated, and Settings "
        "can hide operator actions to force a read-only surface. See "
        "[Local MCP](local-mcp.md) for the runtime model and client "
        "registration.",
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
        lines.append(f"*{question}*")
        lines.append("")
        lines.append(description)
        rows = schema_rows(tool.get("inputSchema") or {})
        if rows:
            lines.append("")
            lines.append("| Parameter | Type | Notes |")
            lines.append("|---|---|---|")
            lines.extend(rows)
        lines.extend(example_block(tool["name"], tool.get("inputSchema") or {}, seeds, cache))
        lines.append("")

    OUT.write_text("\n".join(lines).rstrip() + "\n")
    sidecar = OUT.with_suffix(".faq.json")
    sidecar.write_text(json.dumps(faq_entries, indent=1, ensure_ascii=False) + "\n")
    print(f"wrote {OUT} ({len(tools)} tools) and {sidecar.name}")


if __name__ == "__main__":
    main()
