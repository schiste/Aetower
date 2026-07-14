#!/usr/bin/env python3
"""Assemble the aetower.dev subpages into the website deploy output.

The home page stays a hand-authored standalone HTML file; every other page
is either repo markdown (converted here, so published docs can never fork
from their repo sources) or an HTML fragment under site/pages/, wrapped in
the shared shell template. The full docs/ directory is auto-discovered and
published under /docs/<slug>/ with a generated, grouped index. Also emits
sitemap.xml.

No third-party dependencies: the markdown subset covers what the repo docs
use (headings, ordered/unordered lists, tables, fenced code, inline code,
bold, italics via *, links, blockquotes).
"""

from __future__ import annotations

import argparse
import datetime
import html
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SITE = ROOT / "site"
DOCS = ROOT / "docs"
BASE_URL = "https://aetower.dev"
GITHUB_BLOB = "https://github.com/schiste/Aetower/blob/master"

# Slug overrides (stem -> slug); everything else uses the file stem.
SLUG_OVERRIDES = {"local-mcp": "mcp"}

# Grouping for the /docs/ index. Unlisted docs land in "More".
DOC_GROUPS = [
    (
        "Using Aetower",
        [
            "getting-started",
            "product-direction",
            "features",
            "tabs-guide",
            "download",
            "known-limitations",
        ],
    ),
    ("Automation & agents", ["local-mcp", "mcp-tools"]),
    (
        "Distribution & release",
        [
            "distribution",
            "direct-download-release",
            "homebrew-release",
            "release-checklist",
            "cloudflare-release-hosting",
            "public-preview-validation",
        ],
    ),
    (
        "Engineering internals",
        [
            "module-breakdown",
            "observability",
            "diagnostics-observability-spec",
            "runtime-profiling",
            "performance-harness",
            "local-ci",
            "design-system",
            "brand-assets",
            "bincode-migration-plan",
        ],
    ),
]


# Curated Q&A per page: rendered as a "Questions this page answers" section
# and baked into FAQPage JSON-LD so answer engines can quote the site
# directly. Keep answers short, honest, and consistent with the page body.
PAGE_FAQS: dict[str, list[tuple[str, str]]] = {
    "/docs/getting-started/": [
        (
            "How do I install Aetower?",
            "Download the signed installer package or DMG from aetower.dev, or use Homebrew: "
            "brew tap aeptus/aetower && brew install --cask aetower. macOS 14+ on Apple silicon.",
        ),
        (
            "What should I check on first run?",
            "Open Settings → Setup and walk the checklist: operator-safe mode for heavy views, "
            "collection behavior, optional Chau7 and MCP exposure, and export privacy tiers.",
        ),
        (
            "How do I reset Aetower's local data?",
            "Settings → Advanced → Reset Aetower local data clears persisted history and "
            "diagnostics and restores safe defaults.",
        ),
    ],
    "/docs/mcp/": [
        (
            "How do AI agents connect to Aetower?",
            "Aetower runs a local MCP server on a Unix socket (~/.aetower/mcp.sock). Agents use "
            "the bundled aetower-mcp helper as a stdio MCP server; the app and server share one "
            "live engine.",
        ),
        (
            "Is Aetower's MCP server read-only?",
            "It exposes read-only tools plus guarded operator actions that are visible to "
            "trusted local clients by default; every action is preview- and approval-gated, "
            "and Settings can hide operator actions to force a read-only surface.",
        ),
        (
            "Does the MCP server work when the app is closed?",
            "Standard cached tools keep working from the last written cache; dynamic profiling "
            "tools return a clear error saying the running app is required.",
        ),
        (
            "How do I register Claude or Codex with Aetower?",
            "Settings offers one-click registration for supported Claude and Codex clients. "
            "Registration is never automatic — you opt in before Aetower writes any client "
            "config.",
        ),
    ],
    "/docs/cli/": [
        (
            "How do I see what's straining my Mac from the terminal?",
            "Run aetower top for the loudest entities by friction, or aetower findings for the "
            "recommendation feed. The app must be running.",
        ),
        (
            "How do I get machine-readable output from aetower?",
            "Add --json to any command; exit codes are scriptable, so it composes with jq and "
            "pipelines.",
        ),
        (
            "How do I install the aetower command?",
            "The installer package links it onto your PATH automatically; the Homebrew tap "
            "declares it as a binary; DMG/ZIP installs add it from Settings → AI Clients or "
            "with aetower install.",
        ),
        (
            "Can I switch Aetower's tabs from a script?",
            "Yes: aetower tab <slug> (or the aetower://tab/<slug> URL scheme) switches the "
            "visible workspace without Accessibility trust or synthetic clicks.",
        ),
    ],
    "/privacy/": [
        (
            "What data does Aetower observe?",
            "Local system and process metadata needed to explain runtime pressure — friction, "
            "CPU, memory, disk, network, wakeups, and energy per entity. It stays on your Mac.",
        ),
        (
            "Does anything leave my Mac?",
            "Not unless you opt in. Telemetry, Fleet advertising, VirusTotal hash lookups, and "
            "provider API calls are all off by default, and Privacy → Outbound Data shows each "
            "channel's live state.",
        ),
        (
            "How do I share diagnostics safely?",
            "Export a privacy-tiered support bundle; the redacted tier is the default and "
            "aetower_support_bundle_manifest previews contents before anything is written.",
        ),
    ],
    "/security/": [
        (
            "How do I report a security vulnerability in Aetower?",
            "Use GitHub's private vulnerability reporting at "
            "github.com/schiste/Aetower/security/advisories/new, and do not publish details "
            "publicly before coordination.",
        ),
    ],
    "/vs/": [
        (
            "What is the best Activity Monitor alternative for Mac?",
            "It depends on the slice: iStat Menus or Stats for polished host gauges, DaisyDisk "
            "for disk visualization, Objective-See's tools for security auditing, and Aetower "
            "(early alpha) if you want entity grouping, history, and AI-agent awareness in one "
            "local-first app.",
        ),
        (
            "Is Aetower free?",
            "Yes — free and open source under AGPL-3.0, for macOS 14+ on Apple silicon. It is "
            "an early alpha, and several tools in its comparison table are more mature at "
            "their individual slices.",
        ),
        (
            "What does no tool on the market do yet?",
            "Per-agent resource attribution at the machine layer: which AI agent sessions run "
            "on a Mac, their kernel-measured energy and inferred GPU share, attributed to the "
            "repository they work on. That is the gap Aetower targets.",
        ),
    ],
    "/vs/activity-monitor/": [
        (
            "Is there an Activity Monitor alternative with history?",
            "Aetower keeps bounded local history with entity-level playback and a narrated "
            "timeline, so a past slowdown is an answerable question. Activity Monitor has no "
            "history.",
        ),
        (
            "Can Activity Monitor track AI agent sessions?",
            "No — it has no concept of agents. Aetower shows agent sessions with inferred GPU "
            "share, unified-memory pressure, and kernel-measured energy, each labeled with its "
            "source.",
        ),
        (
            "Do I still need Activity Monitor if I use Aetower?",
            "For a quick force-quit it's fine and always there. Aetower adds entity grouping, "
            "friction scoring, history, storage reclaim, and an agent-readable interface.",
        ),
    ],
    "/ai-agent-monitoring/": [
        (
            "How do I monitor Claude Code's resource usage on a Mac?",
            "Aetower shows each agent as an entity with a friction score, kernel-measured "
            "energy, and — with the Chau7 adapter — per-session repo, branch, and resource "
            "detail.",
        ),
        (
            "How much energy do local LLMs like Ollama use?",
            "Aetower reports kernel-measured per-entity energy and inferred GPU share for local "
            "model servers, labeled as estimates with their source — not exact metering.",
        ),
        (
            "Can my AI agent check what is straining my Mac?",
            "Yes — Aetower's local MCP server exposes 46 tools; one "
            "aetower_top_findings or aetower_investigation_bundle call gives an agent the "
            "ranked answer.",
        ),
    ],
}


def faq_jsonld(entries: list[tuple[str, str]]) -> str:
    import json as _json

    payload = {
        "@context": "https://schema.org",
        "@type": "FAQPage",
        "mainEntity": [
            {
                "@type": "Question",
                "name": q,
                "acceptedAnswer": {"@type": "Answer", "text": a},
            }
            for q, a in entries
        ],
    }
    return (
        '    <script type="application/ld+json">\n'
        + _json.dumps(payload, indent=2, ensure_ascii=False)
        + "\n    </script>\n"
    )


def faq_section(entries: list[tuple[str, str]]) -> str:
    parts = ["<h2>Questions this page answers</h2>"]
    for q, a in entries:
        parts.append(f"<h3>{html.escape(q, quote=False)}</h3>")
        parts.append(f"<p>{html.escape(a, quote=False)}</p>")
    return "\n".join(parts)


INLINE_CODE = re.compile(r"`([^`]+)`")
BOLD = re.compile(r"\*\*([^*]+)\*\*")
ITALIC = re.compile(r"(?<!\*)\*([^*\s][^*]*)\*(?!\*)")
LINK = re.compile(r"\[([^\]]+)\]\(([^)\s]+)\)")
TABLE_SEPARATOR = re.compile(r"^\s*\|?[\s:|-]+\|[\s:|-]*$")


def doc_slug(stem: str) -> str:
    return SLUG_OVERRIDES.get(stem, stem)


def doc_url(stem: str) -> str:
    return f"/docs/{doc_slug(stem)}/"


def published_md_url(name: str) -> str | None:
    """URL for a repo markdown file if it is published on the site."""
    stem = name[:-3] if name.endswith(".md") else name
    if (DOCS / f"{stem}.md").exists():
        return doc_url(stem)
    if stem == "PRIVACY":
        return "/privacy/"
    if stem == "SECURITY":
        return "/security/"
    if stem == "CHANGELOG":
        return "/changelog/"
    return None


def rewrite_href(href: str, source: pathlib.Path) -> str:
    if href.startswith(("http://", "https://", "/", "#", "mailto:")):
        return href
    name = href.split("/")[-1]
    published = published_md_url(name)
    if published:
        return published
    resolved = (source.parent / href).resolve()
    try:
        rel = resolved.relative_to(ROOT)
    except ValueError:
        return href
    return f"{GITHUB_BLOB}/{rel}"


def render_inline(text: str, source: pathlib.Path) -> str:
    text = html.escape(text, quote=False)

    def link_sub(match: re.Match[str]) -> str:
        href = rewrite_href(match.group(2), source)
        return f'<a href="{html.escape(href, quote=True)}">{match.group(1)}</a>'

    text = LINK.sub(link_sub, text)
    text = BOLD.sub(r"<strong>\1</strong>", text)
    text = ITALIC.sub(r"<em>\1</em>", text)
    text = INLINE_CODE.sub(r"<code>\1</code>", text)
    return text


def split_table_row(row: str) -> list[str]:
    return [cell.strip() for cell in row.strip().strip("|").split("|")]


def markdown_to_html(source: pathlib.Path) -> str:
    lines = source.read_text().splitlines()
    out: list[str] = []
    in_code = False
    paragraph: list[str] = []
    list_stack: list[str] = []  # "ul" / "ol"
    table: list[list[str]] | None = None

    def flush_paragraph() -> None:
        if paragraph:
            out.append(f"<p>{render_inline(' '.join(paragraph), source)}</p>")
            paragraph.clear()

    def close_lists(to_depth: int = 0) -> None:
        while len(list_stack) > to_depth:
            out.append(f"</{list_stack.pop()}>")

    def flush_table() -> None:
        nonlocal table
        if table is None:
            return
        out.append('<div class="table-scroll"><table>')
        out.append(
            "<thead><tr>"
            + "".join(f"<th>{render_inline(c, source)}</th>" for c in table[0])
            + "</tr></thead>"
        )
        out.append("<tbody>")
        for row in table[1:]:
            out.append(
                "<tr>" + "".join(f"<td>{render_inline(c, source)}</td>" for c in row) + "</tr>"
            )
        out.append("</tbody></table></div>")
        table = None

    for index, raw in enumerate(lines):
        if raw.strip().startswith("```"):
            flush_paragraph()
            flush_table()
            close_lists()
            out.append("<pre><code>" if not in_code else "</code></pre>")
            in_code = not in_code
            continue
        if in_code:
            out.append(html.escape(raw))
            continue

        stripped = raw.strip()

        if stripped.startswith("|") or (table is not None and "|" in stripped and stripped):
            flush_paragraph()
            close_lists()
            if TABLE_SEPARATOR.match(stripped):
                continue
            if table is None:
                table = []
            table.append(split_table_row(stripped))
            continue
        flush_table()

        heading = re.match(r"^(#{1,4})\s+(.*)$", stripped)
        bullet = re.match(r"^(\s*)-\s+(.*)$", raw)
        ordered = re.match(r"^(\s*)\d+\.\s+(.*)$", raw)

        if not stripped:
            flush_paragraph()
            close_lists()
        elif heading:
            flush_paragraph()
            close_lists()
            level = len(heading.group(1))
            out.append(f"<h{level}>{render_inline(heading.group(2), source)}</h{level}>")
        elif bullet or ordered:
            flush_paragraph()
            match = bullet or ordered
            kind = "ul" if bullet else "ol"
            depth = len(match.group(1)) // 2 + 1
            while len(list_stack) < depth:
                out.append(f"<{kind}>")
                list_stack.append(kind)
            close_lists(depth)
            if list_stack and list_stack[-1] != kind and depth == len(list_stack):
                out.append(f"</{list_stack.pop()}>")
                out.append(f"<{kind}>")
                list_stack.append(kind)
            out.append(f"<li>{render_inline(match.group(2), source)}</li>")
        elif stripped.startswith(">"):
            flush_paragraph()
            close_lists()
            out.append(f"<blockquote>{render_inline(stripped.lstrip('> '), source)}</blockquote>")
        else:
            if list_stack and raw.startswith(("  ", "\t")):
                out[-1] = out[-1][:-5] + " " + render_inline(stripped, source) + "</li>"
            else:
                close_lists()
                paragraph.append(stripped)

    flush_paragraph()
    flush_table()
    close_lists()
    if in_code:
        out.append("</code></pre>")
    return "\n".join(out)


def doc_metadata(source: pathlib.Path) -> tuple[str, str]:
    """(title, description) from the first H1 and first paragraph."""
    title = source.stem.replace("-", " ").title()
    description = ""
    for line in source.read_text().splitlines():
        stripped = line.strip()
        if stripped.startswith("# "):
            title = stripped[2:].strip()
        elif stripped and not stripped.startswith(("#", ">", "-", "|", "```")):
            description = re.sub(r"[*`\[\]]", "", stripped)
            break
    if len(description) > 155:
        description = description[:152].rstrip() + "…"
    return title, description


def docs_index_html(entries: dict[str, tuple[str, str, str]]) -> str:
    """entries: stem -> (url, title, description)."""
    parts = [
        "<h1>Aetower documentation</h1>",
        "<p>Rendered from the repository's <code>docs/</code> directory on every deploy —"
        " the <a href=\"https://github.com/schiste/Aetower/tree/master/docs\">repo</a>"
        " is the source of truth. Engineering documents may lag the app between"
        " releases.</p>",
    ]
    listed: set[str] = set()
    for _, stems in DOC_GROUPS:
        listed.update(stems)
    more = [stem for stem in sorted(entries) if stem not in listed]
    groups = list(DOC_GROUPS) + [("More", more)]
    for group_title, stems in groups:
        items = []
        for stem in stems:
            if stem not in entries:
                continue
            url, title, description = entries[stem]
            suffix = f" — {description}" if description else ""
            items.append(f'<li><a href="{url}">{title}</a>{suffix}</li>')
        if group_title == "More":
            extra = [
                '<li><a href="/docs/cli/">aetower CLI reference</a>'
                " — commands, options, install paths, and driving the UI from scripts</li>",
                '<li><a href="/privacy/">Privacy</a> — what Aetower observes and what stays local</li>',
                '<li><a href="/security/">Security</a> — reporting and hardening posture</li>',
            ]
            items = extra + items
            group_title = "Also on this site"
        if items:
            parts.append(f"<h2>{group_title}</h2>")
            parts.append("<ul>")
            parts.extend(items)
            parts.append("</ul>")
    return "\n".join(parts)


def build(output_dir: pathlib.Path) -> None:
    shell = (SITE / "templates" / "shell.html").read_text()
    today = datetime.date.today().isoformat()
    pages: list[tuple[str, str, str, str, str]] = []  # path, title, desc, eyebrow, body

    # Explicit pages.
    pages.append(
        (
            "/changelog/",
            "Changelog · Aetower",
            "Every notable public change to Aetower, the operator console for Macs that run AI agents.",
            "Releases",
            markdown_to_html(ROOT / "CHANGELOG.md"),
        )
    )
    pages.append(
        (
            "/privacy/",
            "Privacy · Aetower",
            "What Aetower observes, what stays on your Mac, and every channel that is off until you opt in.",
            "Trust",
            markdown_to_html(ROOT / "PRIVACY.md"),
        )
    )
    pages.append(
        (
            "/security/",
            "Security · Aetower",
            "Aetower's security posture and how to report vulnerabilities.",
            "Trust",
            markdown_to_html(ROOT / "SECURITY.md"),
        )
    )
    pages.append(
        (
            "/docs/cli/",
            "aetower CLI · Aetower docs",
            "The aetower command line tool: live friction, storage, and repository state from a running Aetower app, with --json for pipelines.",
            "Docs",
            markdown_to_html(SITE / "pages" / "cli.md"),
        )
    )
    pages.append(
        (
            "/vs/",
            "Aetower vs the field · an honest capability matrix",
            "How Aetower compares to Activity Monitor, iStat Menus, DaisyDisk, osquery, Objective-See, Langfuse, and 15+ other tools — gaps marked on all sides, including ours.",
            "Comparison",
            (SITE / "pages" / "comparisons.html").read_text(),
        )
    )
    pages.append(
        (
            "/vs/activity-monitor/",
            "Aetower vs Activity Monitor · an honest comparison",
            "What macOS Activity Monitor does well, where it stops, and what Aetower adds: entity grouping, friction scoring, history, and an agent-readable interface.",
            "Comparison",
            (SITE / "pages" / "vs-activity-monitor.html").read_text(),
        )
    )
    pages.append(
        (
            "/ai-agent-monitoring/",
            "Monitor AI agents on your Mac · Aetower",
            "See what Claude Code, Codex, Ollama, and other local AI tools cost your Mac: inferred GPU share, unified-memory pressure, kernel-measured energy, and per-repo spend.",
            "Use case",
            (SITE / "pages" / "ai-agent-monitoring.html").read_text(),
        )
    )

    # Auto-discovered repo docs.
    index_entries: dict[str, tuple[str, str, str]] = {}
    for source in sorted(DOCS.glob("*.md")):
        stem = source.stem
        title, description = doc_metadata(source)
        url = doc_url(stem)
        index_entries[stem] = (url, title, description)
        pages.append(
            (
                url,
                f"{title} · Aetower docs",
                description or f"Aetower documentation: {title}.",
                "Docs",
                markdown_to_html(source),
            )
        )

    pages.append(
        (
            "/docs/",
            "Documentation · Aetower",
            "All Aetower documentation: getting started, MCP tool reference, CLI, release engineering, and internals.",
            "Docs",
            docs_index_html(index_entries),
        )
    )

    sitemap_entries = [f"  <url><loc>{BASE_URL}/</loc><lastmod>{today}</lastmod></url>"]
    tools_faq_path = DOCS / "mcp-tools.faq.json"
    for path, title, description, eyebrow, body in pages:
        faqs = list(PAGE_FAQS.get(path, []))
        extra_head = ""
        if faqs:
            body = body + "\n" + faq_section(faqs)
        if path == "/docs/mcp-tools/" and tools_faq_path.exists():
            import json as _json

            tool_faqs = [
                (entry["q"], entry["a"]) for entry in _json.loads(tools_faq_path.read_text())
            ]
            faqs.extend(tool_faqs)
        if faqs:
            extra_head = faq_jsonld(faqs)
        content = f'        <p class="page-eyebrow">{eyebrow}</p>\n{body}'
        page = (
            shell.replace("{{TITLE}}", html.escape(title, quote=True))
            .replace("{{DESCRIPTION}}", html.escape(description, quote=True))
            .replace("{{PATH}}", path)
            .replace("{{EXTRA_HEAD}}", extra_head)
            .replace("{{CONTENT}}", content)
        )
        target = output_dir / path.strip("/") / "index.html"
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(page)
        sitemap_entries.append(
            f"  <url><loc>{BASE_URL}{path}</loc><lastmod>{today}</lastmod></url>"
        )
    print(f"built {len(pages)} pages")

    sitemap = (
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n'
        + "\n".join(sitemap_entries)
        + "\n</urlset>\n"
    )
    (output_dir / "sitemap.xml").write_text(sitemap)
    print("built /sitemap.xml")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=pathlib.Path)
    args = parser.parse_args()
    build(args.output)
