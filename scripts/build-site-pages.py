#!/usr/bin/env python3
"""Assemble the aetower.dev subpages into the website deploy output.

The home page stays a hand-authored standalone HTML file; every other page
is either a repo markdown file (converted here, so docs never fork from the
site) or an HTML fragment under site/pages/, wrapped in the shared shell
template. Also emits sitemap.xml.

No third-party dependencies: the markdown subset covers what the repo docs
use (headings, lists, fenced code, inline code, bold, links, blockquotes).
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
BASE_URL = "https://aetower.dev"
GITHUB_BLOB = "https://github.com/schiste/Aetower/blob/master"

# Repo markdown links that resolve to published pages instead of GitHub.
PUBLISHED_DOC_LINKS = {
    "local-mcp.md": "/docs/mcp/",
    "getting-started.md": "/docs/",
}

# path -> (source, title, description, eyebrow)
PAGES = [
    (
        "/changelog/",
        ROOT / "CHANGELOG.md",
        "Changelog · Aetower",
        "Every notable public change to Aetower, the operator console for Macs that run AI agents.",
        "Releases",
    ),
    (
        "/docs/",
        ROOT / "docs" / "getting-started.md",
        "Getting started · Aetower docs",
        "Install Aetower, walk the first-run checklist, and point local AI agents at its MCP server.",
        "Docs",
    ),
    (
        "/docs/mcp/",
        ROOT / "docs" / "local-mcp.md",
        "Local MCP server · Aetower docs",
        "Aetower's local MCP server: read-only by default, 30+ tools for live system state, history, diagnostics, storage, and repositories.",
        "Docs",
    ),
    (
        "/docs/cli/",
        SITE / "pages" / "cli.md",
        "aetower CLI · Aetower docs",
        "The aetower command line tool: live friction, storage, and repository state from a running Aetower app, with --json for pipelines.",
        "Docs",
    ),
    (
        "/vs/activity-monitor/",
        SITE / "pages" / "vs-activity-monitor.html",
        "Aetower vs Activity Monitor · an honest comparison",
        "What macOS Activity Monitor does well, where it stops, and what Aetower adds: entity grouping, friction scoring, history, and an agent-readable interface.",
        "Comparison",
    ),
    (
        "/ai-agent-monitoring/",
        SITE / "pages" / "ai-agent-monitoring.html",
        "Monitor AI agents on your Mac · Aetower",
        "See what Claude Code, Codex, Ollama, and other local AI tools cost your Mac: inferred GPU share, unified-memory pressure, kernel-measured energy, and per-repo spend.",
        "Use case",
    ),
]

INLINE_CODE = re.compile(r"`([^`]+)`")
BOLD = re.compile(r"\*\*([^*]+)\*\*")
LINK = re.compile(r"\[([^\]]+)\]\(([^)\s]+)\)")


def rewrite_href(href: str, source: pathlib.Path) -> str:
    if href.startswith(("http://", "https://", "/", "#", "mailto:")):
        return href
    name = href.split("/")[-1]
    if name in PUBLISHED_DOC_LINKS:
        return PUBLISHED_DOC_LINKS[name]
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
    text = INLINE_CODE.sub(r"<code>\1</code>", text)
    return text


def markdown_to_html(source: pathlib.Path) -> str:
    lines = source.read_text().splitlines()
    out: list[str] = []
    list_depth = 0
    in_code = False
    paragraph: list[str] = []

    def flush_paragraph() -> None:
        if paragraph:
            out.append(f"<p>{render_inline(' '.join(paragraph), source)}</p>")
            paragraph.clear()

    def close_lists(to_depth: int = 0) -> None:
        nonlocal list_depth
        while list_depth > to_depth:
            out.append("</ul>")
            list_depth -= 1

    for raw in lines:
        if raw.strip().startswith("```"):
            flush_paragraph()
            close_lists()
            if in_code:
                out.append("</code></pre>")
            else:
                out.append("<pre><code>")
            in_code = not in_code
            continue
        if in_code:
            out.append(html.escape(raw))
            continue

        stripped = raw.strip()
        heading = re.match(r"^(#{1,4})\s+(.*)$", stripped)
        bullet = re.match(r"^(\s*)-\s+(.*)$", raw)

        if not stripped:
            flush_paragraph()
            close_lists()
        elif heading:
            flush_paragraph()
            close_lists()
            level = len(heading.group(1))
            out.append(f"<h{level}>{render_inline(heading.group(2), source)}</h{level}>")
        elif bullet:
            flush_paragraph()
            depth = len(bullet.group(1)) // 2 + 1
            while list_depth < depth:
                out.append("<ul>")
                list_depth += 1
            close_lists(depth)
            out.append(f"<li>{render_inline(bullet.group(2), source)}</li>")
        elif stripped.startswith(">"):
            flush_paragraph()
            close_lists()
            out.append(f"<blockquote>{render_inline(stripped.lstrip('> '), source)}</blockquote>")
        else:
            if list_depth and raw.startswith(("  ", "\t")):
                # continuation of a wrapped list item
                out[-1] = out[-1][:-5] + " " + render_inline(stripped, source) + "</li>"
            else:
                close_lists()
                paragraph.append(stripped)

    flush_paragraph()
    close_lists()
    if in_code:
        out.append("</code></pre>")
    return "\n".join(out)


def build(output_dir: pathlib.Path) -> None:
    shell = (SITE / "templates" / "shell.html").read_text()
    today = datetime.date.today().isoformat()
    sitemap_entries = [f"  <url><loc>{BASE_URL}/</loc><lastmod>{today}</lastmod></url>"]

    for path, source, title, description, eyebrow in PAGES:
        if not source.exists():
            print(f"error: missing page source {source}", file=sys.stderr)
            raise SystemExit(1)
        if source.suffix == ".md":
            body = markdown_to_html(source)
        else:
            body = source.read_text()
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
        print(f"built {path}")

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
