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
        ["getting-started", "features", "tabs-guide", "download", "known-limitations"],
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
    for path, title, description, eyebrow, body in pages:
        content = f'        <p class="page-eyebrow">{eyebrow}</p>\n{body}'
        page = (
            shell.replace("{{TITLE}}", html.escape(title, quote=True))
            .replace("{{DESCRIPTION}}", html.escape(description, quote=True))
            .replace("{{PATH}}", path)
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
