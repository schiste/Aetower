#!/usr/bin/env python3
from __future__ import annotations

import argparse
import pathlib
import re
import subprocess
import sys
from collections import defaultdict
from dataclasses import dataclass


ROOT = pathlib.Path(__file__).resolve().parents[1]
GENERATED_FILES = {
    "macos/Sources/AetowerBindings/aetower_ffi.swift",
    "macos/Sources/aetower_ffiFFI/aetower_ffiFFI.h",
}
SOURCE_EXTENSIONS = {".swift", ".rs", ".sh"}
PLACEHOLDER_EXTENSIONS = {".swift", ".rs", ".sh", ".toml", ".yml", ".yaml", ".json"}
SWIFT_PUBLIC_VIEW_ALLOWLIST = {
    "Chau7View.swift",
    "CompactHUDView.swift",
    "DiagnosticsView.swift",
    "EntityDetailView.swift",
    "FleetView.swift",
    "HistoryView.swift",
    "MainListView.swift",
    "MenuBarSummaryView.swift",
    "OverviewView.swift",
    "ProcessTreeView.swift",
    "SettingsView.swift",
    "TimelineView.swift",
}
SUSPICIOUS_NEW_FILE_NAMES = {
    "Utils.swift",
    "Helpers.swift",
    "Common.swift",
    "Misc.swift",
    "Extensions.swift",
    "Manager.swift",
    "utils.rs",
    "helpers.rs",
    "common.rs",
    "misc.rs",
    "util.rs",
}
SWIFT_COLOR_LITERAL = re.compile(
    r"\bColor\((?!secondary|primary)|\bColor\.(red|green|blue|orange|yellow|purple|pink|teal|mint|cyan|indigo|brown)\b"
)
SECRET_PATTERNS = [
    re.compile(r"AKIA[0-9A-Z]{16}"),
    re.compile(r"-----BEGIN (RSA|EC|OPENSSH|DSA|PGP) PRIVATE KEY-----"),
    re.compile(r"(?i)\b(api[_-]?key|secret|token|password)\b.{0,20}[:=].{0,5}[\"'][^\"']{8,}[\"']"),
    re.compile(r"ghp_[A-Za-z0-9]{30,}"),
]
PLACEHOLDER_PATTERN = re.compile(r"\b(TODO|FIXME|XXX|HACK|stub|placeholder)\b", re.IGNORECASE)
SWIFT_FORBIDDEN_PATTERNS = [
    (re.compile(r"\bAnyView\b"), "Avoid AnyView in app code."),
    (re.compile(r"\bas!\b"), "Avoid force casts in Swift UI code."),
    (re.compile(r"\btry!\b"), "Avoid try! in Swift UI code."),
    (re.compile(r"\bfatalError\("), "Avoid fatalError in production Swift code."),
]
RUST_FORBIDDEN_PATTERNS = [
    (re.compile(r"\btodo!\b"), "Do not commit todo! placeholders."),
    (re.compile(r"\bunimplemented!\b"), "Do not commit unimplemented! placeholders."),
    (re.compile(r"\bdbg!\b"), "Do not commit dbg! calls."),
]
RUST_UNWRAP_PATTERN = re.compile(r"\.(unwrap|expect)\(")
PACKAGE_SWIFT_REMOTE_DEP_PATTERN = re.compile(r"\.package\s*\(\s*url:")
DEPENDENCY_RISK_PATTERN = re.compile(r"\b(git|branch|rev|tag)\s*=")
WILDCARD_VERSION_PATTERN = re.compile(r'version\s*=\s*"\*"')


@dataclass
class Violation:
    path: str
    line: int | None
    message: str


def run_git(*args: str, check: bool = True) -> str:
    result = subprocess.run(
        ["git", *args],
        cwd=ROOT,
        text=True,
        capture_output=True,
    )
    if check and result.returncode != 0:
        raise RuntimeError(result.stderr.strip() or f"git {' '.join(args)} failed")
    return result.stdout


def existing_ref(ref: str) -> bool:
    result = subprocess.run(
        ["git", "rev-parse", "--verify", ref],
        cwd=ROOT,
        text=True,
        capture_output=True,
    )
    return result.returncode == 0


def default_diff_base() -> str:
    if existing_ref("HEAD~1"):
        return "HEAD~1"
    if existing_ref("@{upstream}"):
        return "@{upstream}"
    return "HEAD"


def parse_name_status(mode: str, diff_base: str | None) -> dict[str, str]:
    if mode == "pre-commit":
        output = run_git("diff", "--cached", "--name-status", "--diff-filter=ACMR")
    elif mode == "working-tree":
        output = run_git("diff", "--name-status", "--diff-filter=ACMR", "HEAD")
    else:
        base = diff_base or default_diff_base()
        output = run_git("diff", "--name-status", "--diff-filter=ACMR", f"{base}...HEAD")
    statuses: dict[str, str] = {}
    for line in output.splitlines():
        if not line.strip():
            continue
        status, path = line.split("\t", 1)
        statuses[path] = status
    return statuses


def parse_added_lines(mode: str, diff_base: str | None) -> dict[str, list[tuple[int, str]]]:
    if mode == "pre-commit":
        output = run_git("diff", "--cached", "--unified=0", "--no-color", "--diff-filter=ACMR")
    elif mode == "working-tree":
        output = run_git("diff", "--unified=0", "--no-color", "--diff-filter=ACMR", "HEAD")
    else:
        base = diff_base or default_diff_base()
        output = run_git("diff", "--unified=0", "--no-color", "--diff-filter=ACMR", f"{base}...HEAD")
    added: dict[str, list[tuple[int, str]]] = defaultdict(list)
    current_path: str | None = None
    current_line = 0
    for raw in output.splitlines():
        if raw.startswith("+++ b/"):
            current_path = raw[6:]
            continue
        if raw.startswith("@@"):
            match = re.search(r"\+(\d+)(?:,(\d+))?", raw)
            if match:
                current_line = int(match.group(1))
            continue
        if current_path is None:
            continue
        if raw.startswith("+") and not raw.startswith("+++"):
            added[current_path].append((current_line, raw[1:]))
            current_line += 1
        elif raw.startswith("-") and not raw.startswith("---"):
            continue
        else:
            current_line += 1
    return added


def staged_or_changed_files(mode: str, diff_base: str | None) -> list[str]:
    statuses = parse_name_status(mode, diff_base)
    return sorted(statuses)


def file_text(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def non_generated_source(path: str) -> bool:
    if path in GENERATED_FILES:
        return False
    if path.startswith(("rust/target/", "macos/.build/", "tmp/")):
        return False
    return pathlib.Path(path).suffix in SOURCE_EXTENSIONS


def is_test_like(path: str) -> bool:
    return "/tests/" in path or path.endswith("_test.swift") or path.endswith("bench.rs")


def check_placeholders_and_secrets(
    added_lines: dict[str, list[tuple[int, str]]], violations: list[Violation]
) -> None:
    for path, entries in added_lines.items():
        if path in GENERATED_FILES:
            continue
        suffix = pathlib.Path(path).suffix
        check_placeholders = not path.startswith(".semgrep/")
        for line_number, line in entries:
            for pattern in SECRET_PATTERNS:
                if pattern.search(line):
                    violations.append(Violation(path, line_number, "Potential secret detected in added line."))
                    break
            if check_placeholders and suffix in PLACEHOLDER_EXTENSIONS and PLACEHOLDER_PATTERN.search(line):
                violations.append(Violation(path, line_number, "Placeholder marker detected in added line."))


def check_new_file_names(mode: str, diff_base: str | None, violations: list[Violation]) -> None:
    statuses = parse_name_status(mode, diff_base)
    for path, status in statuses.items():
        if status != "A":
            continue
        name = pathlib.Path(path).name
        if name in SUSPICIOUS_NEW_FILE_NAMES:
            violations.append(Violation(path, None, "Suspicious dumping-ground filename; use a specific name instead."))


def check_swift_added_lines(
    added_lines: dict[str, list[tuple[int, str]]], violations: list[Violation]
) -> None:
    for path, entries in added_lines.items():
        if not path.endswith(".swift") or path in GENERATED_FILES:
            continue
        for line_number, line in entries:
            for pattern, message in SWIFT_FORBIDDEN_PATTERNS:
                if pattern.search(line):
                    violations.append(Violation(path, line_number, message))
            if path.startswith("macos/Sources/AetowerUI/") and pathlib.Path(path).name != "DesignTokens.swift":
                if SWIFT_COLOR_LITERAL.search(line):
                    violations.append(Violation(path, line_number, "Use AetowerDesign tokens instead of hardcoded SwiftUI colors."))
            if (
                path.startswith("macos/Sources/AetowerUI/")
                and re.search(r"\bpublic struct \w+View: View\b", line)
                and pathlib.Path(path).name not in SWIFT_PUBLIC_VIEW_ALLOWLIST
            ):
                violations.append(Violation(path, line_number, "Do not export reusable-looking app-level views outside approved surfaces."))


def check_textfield_modifier(
    added_lines: dict[str, list[tuple[int, str]]], violations: list[Violation]
) -> None:
    for path, entries in added_lines.items():
        if not path.startswith("macos/Sources/AetowerUI/") or not path.endswith(".swift"):
            continue
        lines = file_text(path).splitlines()
        for line_number, line in entries:
            if "TextField(" not in line:
                continue
            window = "\n".join(lines[line_number - 1 : min(len(lines), line_number + 8)])
            if ".utilityTextInput()" not in window and ".aetowerUtilityTextInput()" not in window:
                violations.append(Violation(path, line_number, "TextField must use .utilityTextInput() in AetowerUI."))


def check_rust_added_lines(
    added_lines: dict[str, list[tuple[int, str]]], violations: list[Violation]
) -> None:
    for path, entries in added_lines.items():
        if not path.endswith(".rs"):
            continue
        allow_panics = is_test_like(path)
        for line_number, line in entries:
            for pattern, message in RUST_FORBIDDEN_PATTERNS:
                if pattern.search(line):
                    violations.append(Violation(path, line_number, message))
            if not allow_panics and RUST_UNWRAP_PATTERN.search(line):
                violations.append(Violation(path, line_number, "Avoid unwrap/expect in production Rust code."))


def check_dependency_hygiene(
    changed_files: list[str],
    added_lines: dict[str, list[tuple[int, str]]],
    mode: str,
    diff_base: str | None,
    violations: list[Violation],
) -> None:
    if any(path.endswith("Cargo.toml") for path in changed_files):
        for path, entries in added_lines.items():
            if not path.endswith("Cargo.toml"):
                continue
            for line_number, line in entries:
                if DEPENDENCY_RISK_PATTERN.search(line):
                    violations.append(Violation(path, line_number, "Do not add git or branch-based Cargo dependencies."))
                if WILDCARD_VERSION_PATTERN.search(line):
                    violations.append(Violation(path, line_number, "Do not add wildcard Cargo dependency versions."))
        if "rust/Cargo.lock" not in changed_files:
            violations.append(Violation("rust/Cargo.lock", None, "Cargo.lock must change when Cargo.toml changes."))

    if "macos/Package.swift" in changed_files:
        for line_number, line in added_lines.get("macos/Package.swift", []):
            if PACKAGE_SWIFT_REMOTE_DEP_PATTERN.search(line):
                violations.append(Violation("macos/Package.swift", line_number, "Do not add remote SwiftPM packages without explicit review."))


def normalize_for_dup(line: str) -> str:
    line = re.sub(r"//.*", "", line)
    line = re.sub(r"#\[.*\]", "", line)
    line = line.strip()
    if not line:
        return ""
    line = re.sub(r"\s+", " ", line)
    return line


def duplicate_candidates(path: str, text: str) -> list[tuple[int, tuple[str, ...]]]:
    normalized: list[tuple[int, str]] = []
    for idx, raw in enumerate(text.splitlines(), start=1):
        line = normalize_for_dup(raw)
        if not line or line in {"{", "}", "};"}:
            continue
        normalized.append((idx, line))
    windows: list[tuple[int, tuple[str, ...]]] = []
    window_size = 8
    for start in range(0, len(normalized) - window_size + 1):
        chunk = normalized[start : start + window_size]
        block = tuple(line for _, line in chunk)
        if len("".join(block)) < 160 or len(set(block)) < 5:
            continue
        windows.append((chunk[0][0], block))
    return windows


def check_duplicate_blocks(
    changed_files: list[str],
    violations: list[Violation],
) -> None:
    candidate_files = [
        path
        for path in staged_or_repo_source_files()
        if path.startswith("macos/Sources/AetowerUI/")
        and non_generated_source(path)
        and not is_test_like(path)
    ]
    if not changed_files:
        return
    index: dict[tuple[str, ...], list[tuple[str, int]]] = defaultdict(list)
    for path in candidate_files:
        text = file_text(path)
        for line_number, block in duplicate_candidates(path, text):
            index[block].append((path, line_number))
    seen: set[tuple[str, int, str, int]] = set()
    for path in changed_files:
        if path not in candidate_files:
            continue
        for line_number, block in duplicate_candidates(path, file_text(path)):
            occurrences = [(other_path, other_line) for other_path, other_line in index[block] if other_path != path]
            if not occurrences:
                continue
            other_path, other_line = occurrences[0]
            key = (path, line_number, other_path, other_line)
            if key in seen:
                continue
            seen.add(key)
            violations.append(
                Violation(
                    path,
                    line_number,
                    f"Duplicate code block also found in {other_path}:{other_line}. Extract or delete the copy-paste.",
                )
            )


def staged_or_repo_source_files() -> list[str]:
    output = run_git("ls-files")
    return [
        line
        for line in output.splitlines()
        if line and non_generated_source(line)
    ]


def shell_check(targets: list[str], violations: list[Violation]) -> None:
    for path in targets:
        if not (path.endswith(".sh") or path.startswith(".githooks/")):
            continue
        result = subprocess.run(["sh", "-n", path], cwd=ROOT, text=True, capture_output=True)
        if result.returncode != 0:
            violations.append(Violation(path, None, result.stderr.strip() or "Shell syntax check failed."))


def print_violations(violations: list[Violation]) -> int:
    if not violations:
        print("quality-guard: no issues found")
        return 0
    print("quality-guard: issues found", file=sys.stderr)
    for item in violations:
        location = f"{item.path}:{item.line}" if item.line else item.path
        print(f" - {location}: {item.message}", file=sys.stderr)
    return 1


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--mode", choices=["pre-commit", "working-tree", "pre-push", "full"], required=True)
    parser.add_argument("--diff-base")
    args = parser.parse_args()

    diff_base = None if args.mode in {"pre-commit", "working-tree"} else (args.diff_base or default_diff_base())
    changed_files = staged_or_changed_files(args.mode, diff_base)
    added_lines = parse_added_lines(args.mode, diff_base)
    violations: list[Violation] = []

    check_placeholders_and_secrets(added_lines, violations)
    check_new_file_names(args.mode, diff_base, violations)
    check_swift_added_lines(added_lines, violations)
    check_textfield_modifier(added_lines, violations)
    check_rust_added_lines(added_lines, violations)
    check_dependency_hygiene(changed_files, added_lines, args.mode, diff_base, violations)
    shell_check(changed_files, violations)

    if args.mode in {"pre-push", "full", "working-tree"}:
        check_duplicate_blocks(changed_files, violations)

    return print_violations(violations)


if __name__ == "__main__":
    sys.exit(main())
