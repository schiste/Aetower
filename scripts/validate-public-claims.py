#!/usr/bin/env python3
"""Validate release-facing Aetower claims against source and artifacts.

This script is intentionally conservative: public website claims should be
backed by code defaults, generated release metadata, or reachable published
assets. It can run in two modes:

  * local/source mode (default): validates source defaults and any local dist/
    artifacts that are present.
  * published mode (--published): additionally validates the public appcast and
    release asset URLs before the website deploys.
"""

from __future__ import annotations

import argparse
import ast
import os
import plistlib
import re
import sys
import urllib.error
import urllib.request
import xml.etree.ElementTree as ET
from dataclasses import dataclass
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
SPARKLE_NS = "http://www.andymatuschak.org/xml-namespaces/sparkle"
EXPECTED_HISTORY_RETENTION_DAYS = 7
EXPECTED_PLATFORM = "arm64"


@dataclass
class CheckResult:
    name: str
    detail: str
    status: str = "PASS"


class ClaimsValidator:
    def __init__(self) -> None:
        self.results: list[CheckResult] = []
        self.failures: list[CheckResult] = []

    def pass_(self, name: str, detail: str) -> None:
        self.results.append(CheckResult(name=name, detail=detail))

    def skip(self, name: str, detail: str) -> None:
        self.results.append(CheckResult(name=name, detail=detail, status="SKIP"))

    def fail(self, name: str, detail: str) -> None:
        result = CheckResult(name=name, detail=detail, status="FAIL")
        self.results.append(result)
        self.failures.append(result)

    def require(self, condition: bool, name: str, detail: str) -> bool:
        if condition:
            self.pass_(name, detail)
            return True
        self.fail(name, detail)
        return False

    def print_summary(self) -> None:
        for result in self.results:
            print(f"{result.status:4} {result.name}: {result.detail}")
        if self.failures:
            print("\nclaims validation failed:", file=sys.stderr)
            for failure in self.failures:
                print(f"  - {failure.name}: {failure.detail}", file=sys.stderr)


def read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def load_release_env(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    if not path.is_file():
        return values
    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        key = key.strip()
        value = value.strip().strip("'\"")
        if key and key not in os.environ:
            values[key] = value
    return values


def env_value(env: dict[str, str], key: str, default: str = "") -> str:
    return os.environ.get(key) or env.get(key) or default


def parse_rust_constants(source: str) -> dict[str, str]:
    return dict(
        re.findall(
            r"^const\s+([A-Z0-9_]+)\s*:\s*[^=]+=\s*([^;]+);",
            source,
            flags=re.MULTILINE,
        )
    )


def evaluate_const(name: str, expressions: dict[str, str], memo: dict[str, int]) -> int:
    if name in memo:
        return memo[name]
    if name not in expressions:
        raise KeyError(name)
    value = evaluate_expr(expressions[name], expressions, memo)
    memo[name] = value
    return value


def evaluate_expr(expr: str, expressions: dict[str, str], memo: dict[str, int]) -> int:
    tree = ast.parse(expr, mode="eval")

    def visit(node: ast.AST) -> int:
        if isinstance(node, ast.Expression):
            return visit(node.body)
        if isinstance(node, ast.Constant) and isinstance(node.value, int):
            return node.value
        if isinstance(node, ast.Name):
            return evaluate_const(node.id, expressions, memo)
        if isinstance(node, ast.BinOp):
            left = visit(node.left)
            right = visit(node.right)
            if isinstance(node.op, ast.Add):
                return left + right
            if isinstance(node.op, ast.Sub):
                return left - right
            if isinstance(node.op, ast.Mult):
                return left * right
            if isinstance(node.op, ast.Div):
                return left // right
            if isinstance(node.op, ast.FloorDiv):
                return left // right
        raise ValueError(f"unsupported const expression: {expr}")

    return visit(tree)


def check_history_retention(validator: ClaimsValidator) -> None:
    source = read_text(ROOT / "rust/crates/aetower-core/src/engine.rs")
    constants = parse_rust_constants(source)
    memo: dict[str, int] = {}
    expected_millis = EXPECTED_HISTORY_RETENTION_DAYS * 86_400_000
    try:
        regression = evaluate_const("REGRESSION_WINDOW_MILLIS", constants, memo)
        retention = evaluate_const("DEFAULT_HISTORY_RETENTION_MILLIS", constants, memo)
        emergency = evaluate_const("EMERGENCY_HISTORY_RETENTION_MILLIS", constants, memo)
        soft_cap = evaluate_const("HISTORY_SOFT_MAX_BYTES", constants, memo)
        hard_cap = evaluate_const("HISTORY_HARD_MAX_BYTES", constants, memo)
        wal_cap = evaluate_const("HISTORY_MAX_WAL_BYTES", constants, memo)
    except Exception as error:  # noqa: BLE001 - report exact validation failure.
        validator.fail("history retention days", f"could not evaluate retention constants: {error}")
        return

    validator.require(
        retention == expected_millis and regression == expected_millis,
        "history retention days",
        f"default={retention // 86_400_000}d regression={regression // 86_400_000}d",
    )
    validator.require(
        0 < emergency < retention and 0 < soft_cap < hard_cap and wal_cap > 0,
        "history storage caps",
        f"emergency={emergency} soft={soft_cap} hard={hard_cap} wal={wal_cap}",
    )


def descriptor_block(source: str, tool_name: str) -> str:
    marker = f'"{tool_name}"'
    start = source.find(marker)
    if start < 0:
        return ""
    descriptor_start = source.rfind("ToolDescriptor::", 0, start)
    next_start = source.find("ToolDescriptor::", start + len(marker))
    if next_start < 0:
        next_start = len(source)
    return source[descriptor_start:next_start]


def check_mcp_default_tool_safety(validator: ClaimsValidator) -> None:
    descriptors = read_text(ROOT / "rust/crates/aetower-mcp/src/tools/descriptors.rs")
    lib = read_text(ROOT / "rust/crates/aetower-mcp/src/lib.rs")
    process_action = descriptor_block(descriptors, "aetower_process_action")

    validator.require(
        "tool_definitions_for_mode(false)" in descriptors
        and "TOOL_LIST_RESULT_READ_ONLY" in descriptors,
        "MCP default list mode",
        "read-only tool list is generated with operator_actions_enabled=false",
    )
    validator.require(
        bool(process_action) and ".operator_action()" in process_action,
        "MCP process action hidden by default",
        "aetower_process_action descriptor is operator-action gated",
    )
    validator.require(
        "default_tools_list_excludes_operator_actions" in lib
        and 'name == "aetower_process_action"' in lib
        and "default_tools_call_hides_process_action_even_by_name" in lib,
        "MCP safety tests present",
        "default tool list and direct call hiding are pinned by Rust tests",
    )


def swift_default_false(source: str, property_name: str) -> bool:
    pattern = (
        rf"self\.{re.escape(property_name)}\s*=\s*defaults\.object"
        rf"\([^)]*{re.escape(property_name)}Key[^)]*\)\s*as\?\s*Bool\s*\?\?\s*false"
    )
    return re.search(pattern, source, flags=re.DOTALL) is not None


def check_outbound_defaults(validator: ClaimsValidator) -> None:
    source = read_text(ROOT / "macos/Sources/AetowerUI/SettingsStore.swift")
    toggles = [
        "telemetryEnabled",
        "binaryReputationEnabled",
        "autoRegisterLocalMcpClientsEnabled",
        "localMcpOperatorActionsEnabled",
        "fleetEnabled",
        "storageScheduledScansEnabled",
        "privilegedHelperEnabled",
    ]
    missing = [name for name in toggles if not swift_default_false(source, name)]
    validator.require(
        not missing,
        "default outbound toggles off",
        "off by default: " + ", ".join(toggles) if not missing else "missing false defaults: " + ", ".join(missing),
    )
    validator.require(
        "?? (legacySensitive ? .full : .redacted)" in source
        and "githubOAuthClientIDKey) ?? \"\"" in source
        and "cloudflareOAuthClientIDKey) ?? \"\"" in source,
        "default outbound credentials empty",
        "privacy tier defaults redacted and provider client IDs default empty",
    )
    validator.require(
        swift_default_false(source, "fleetEnabled"),
        "Fleet default off",
        "settings.fleetEnabled defaults to false",
    )


@dataclass
class ReleaseContext:
    dist_dir: Path
    app_dir: Path
    appcast_path: Path
    appcast_dir: Path
    source_dir: Path
    public_base_url: str
    appcast_url: str
    expected_version: str | None
    expected_build: str | None
    expected_min_system: str | None


def plist_value(metadata: dict[str, Any], key: str) -> str | None:
    value = metadata.get(key)
    return str(value) if value is not None else None


def app_metadata(app_dir: Path) -> dict[str, str | None]:
    plist_path = app_dir / "Contents/Info.plist"
    with plist_path.open("rb") as handle:
        plist = plistlib.load(handle)
    return {
        "version": plist_value(plist, "CFBundleShortVersionString"),
        "build": plist_value(plist, "CFBundleVersion"),
        "feed_url": plist_value(plist, "SUFeedURL"),
        "min_system": plist_value(plist, "LSMinimumSystemVersion"),
    }


def release_context(env: dict[str, str]) -> ReleaseContext:
    dist_dir = Path(env_value(env, "AETOWER_DIST_DIR", str(ROOT / "dist")))
    app_dir = Path(env_value(env, "AETOWER_APP_DIR", str(dist_dir / "Aetower.app")))
    appcast_dir = Path(env_value(env, "AETOWER_APPCAST_DIR", str(dist_dir / "appcast")))
    appcast_path = Path(env_value(env, "AETOWER_APPCAST_PATH", str(appcast_dir / "appcast.xml")))
    source_dir = Path(env_value(env, "AETOWER_SOURCE_ARCHIVE_DIR", str(dist_dir / "source")))

    metadata: dict[str, str | None] = {}
    if (app_dir / "Contents/Info.plist").is_file():
        metadata = app_metadata(app_dir)

    expected_version = env_value(env, "AETOWER_VERSION") or metadata.get("version")
    expected_build = env_value(env, "AETOWER_BUILD_NUMBER") or metadata.get("build")
    expected_min_system = metadata.get("min_system")
    appcast_url = env_value(env, "AETOWER_APPCAST_URL") or metadata.get("feed_url") or ""
    public_base_url = env_value(env, "AETOWER_PUBLIC_BASE_URL")
    if not public_base_url and appcast_url.endswith("/releases/appcast.xml"):
        public_base_url = appcast_url[: -len("/releases/appcast.xml")]
    if not public_base_url:
        public_base_url = "https://aetower.dev"
    if not appcast_url:
        appcast_url = public_base_url.rstrip("/") + "/releases/appcast.xml"

    return ReleaseContext(
        dist_dir=dist_dir,
        app_dir=app_dir,
        appcast_path=appcast_path,
        appcast_dir=appcast_dir,
        source_dir=source_dir,
        public_base_url=public_base_url.rstrip("/"),
        appcast_url=appcast_url,
        expected_version=expected_version,
        expected_build=expected_build,
        expected_min_system=expected_min_system,
    )


@dataclass
class AppcastMetadata:
    version: str
    build: str
    min_system: str
    hardware: str
    enclosure_url: str
    enclosure_length: int | None
    ed_signature: str


def parse_appcast(data: bytes) -> AppcastMetadata:
    root = ET.fromstring(data)
    ns = {"sparkle": SPARKLE_NS}
    item = root.find("./channel/item")
    if item is None:
        raise ValueError("missing channel/item")
    enclosure = item.find("enclosure")
    if enclosure is None:
        raise ValueError("missing enclosure")

    def text(tag: str) -> str:
        value = item.findtext(tag, namespaces=ns)
        return (value or "").strip()

    length_raw = enclosure.attrib.get("length", "").strip()
    return AppcastMetadata(
        version=text("sparkle:shortVersionString"),
        build=text("sparkle:version"),
        min_system=text("sparkle:minimumSystemVersion"),
        hardware=text("sparkle:hardwareRequirements"),
        enclosure_url=enclosure.attrib.get("url", "").strip(),
        enclosure_length=int(length_raw) if length_raw.isdigit() else None,
        ed_signature=enclosure.attrib.get(f"{{{SPARKLE_NS}}}edSignature", "").strip(),
    )


def validate_appcast_metadata(
    validator: ClaimsValidator,
    appcast: AppcastMetadata,
    context: ReleaseContext,
    label: str,
) -> None:
    exact_version_ok = not context.expected_version or appcast.version == context.expected_version
    exact_build_ok = not context.expected_build or appcast.build == context.expected_build
    min_system_ok = not context.expected_min_system or appcast.min_system == context.expected_min_system
    validator.require(
        bool(appcast.version)
        and bool(appcast.build)
        and exact_version_ok
        and exact_build_ok
        and appcast.hardware == EXPECTED_PLATFORM
        and min_system_ok,
        f"{label} appcast version/build/platform",
        (
            f"version={appcast.version} build={appcast.build} "
            f"hardware={appcast.hardware} min_system={appcast.min_system}"
        ),
    )
    validator.require(
        bool(appcast.enclosure_url) and bool(appcast.ed_signature),
        f"{label} appcast signed archive",
        f"archive={appcast.enclosure_url or 'missing'} signature={'present' if appcast.ed_signature else 'missing'}",
    )


def check_cli_bundled(validator: ClaimsValidator, context: ReleaseContext) -> None:
    package_script = read_text(ROOT / "scripts/package-macos.sh")
    source_contract_ok = (
        'cp "$ROOT/rust/target/release/aetower" "$HELPER_DIR/aetower"' in package_script
        and 'sign_target "$HELPER_DIR/aetower" runtime' in package_script
    )
    if context.app_dir.is_dir():
        cli = context.app_dir / "Contents/Helpers/aetower"
        validator.require(
            cli.is_file() and os.access(cli, os.X_OK),
            "CLI binary bundled",
            str(cli),
        )
    else:
        validator.skip("CLI binary bundled", f"local app bundle absent: {context.app_dir}")
    validator.require(
        source_contract_ok,
        "CLI packaging contract",
        "package-macos.sh copies and signs Contents/Helpers/aetower",
    )


def check_local_release_artifacts(
    validator: ClaimsValidator,
    context: ReleaseContext,
    require_local_release: bool,
) -> None:
    local_artifacts_present = context.app_dir.is_dir() or context.appcast_path.is_file()
    if not local_artifacts_present and not require_local_release:
        validator.skip("local release artifacts", "dist/Aetower.app and dist/appcast/appcast.xml are absent")
        check_cli_bundled(validator, context)
        return

    if not validator.require(context.app_dir.is_dir(), "local app bundle", str(context.app_dir)):
        return
    info_plist = context.app_dir / "Contents/Info.plist"
    if not validator.require(info_plist.is_file(), "local app metadata", str(info_plist)):
        return
    metadata = app_metadata(context.app_dir)
    validator.require(
        bool(metadata["version"]) and bool(metadata["build"]) and str(metadata["feed_url"] or "").startswith("https://"),
        "local app release metadata",
        f"version={metadata['version']} build={metadata['build']} feed={metadata['feed_url']}",
    )

    check_cli_bundled(validator, context)

    if not validator.require(context.appcast_path.is_file(), "local appcast exists", str(context.appcast_path)):
        return
    try:
        appcast = parse_appcast(context.appcast_path.read_bytes())
    except Exception as error:  # noqa: BLE001 - report exact validation failure.
        validator.fail("local appcast parse", str(error))
        return
    validate_appcast_metadata(validator, appcast, context, "local")

    archive_path = context.appcast_dir / Path(appcast.enclosure_url).name
    length_ok = archive_path.is_file() and (
        appcast.enclosure_length is None or archive_path.stat().st_size == appcast.enclosure_length
    )
    validator.require(
        length_ok,
        "local appcast archive exists",
        f"{archive_path} length={appcast.enclosure_length}",
    )

    if context.expected_version and context.expected_build:
        versioned_source = context.source_dir / f"Aetower-{context.expected_version}-{context.expected_build}-source.tar.gz"
        latest_source = context.source_dir / "Aetower-source.tar.gz"
        validator.require(
            versioned_source.is_file() and latest_source.is_file(),
            "source archive exists",
            f"{versioned_source.name} and {latest_source.name}",
        )
        validator.require(
            (Path(str(versioned_source) + ".sha256")).is_file()
            and (Path(str(latest_source) + ".sha256")).is_file(),
            "source archive checksums exist",
            "versioned and latest .sha256 files",
        )
    else:
        validator.fail("source archive exists", "expected version/build unavailable")


def fetch_url(url: str, timeout: float) -> bytes:
    request = urllib.request.Request(url, method="GET", headers={"User-Agent": "aetower-claims-validator/1"})
    with urllib.request.urlopen(request, timeout=timeout) as response:  # noqa: S310 - release gate fetches configured URLs.
        return response.read()


def check_url_reachable(validator: ClaimsValidator, name: str, url: str, timeout: float) -> bool:
    try:
        request = urllib.request.Request(url, method="HEAD", headers={"User-Agent": "aetower-claims-validator/1"})
        with urllib.request.urlopen(request, timeout=timeout) as response:  # noqa: S310
            status = response.status
        return validator.require(200 <= status < 400, name, f"{status} {url}")
    except (urllib.error.HTTPError, urllib.error.URLError, TimeoutError):
        try:
            request = urllib.request.Request(
                url,
                method="GET",
                headers={"User-Agent": "aetower-claims-validator/1", "Range": "bytes=0-0"},
            )
            with urllib.request.urlopen(request, timeout=timeout) as response:  # noqa: S310
                status = response.status
            return validator.require(200 <= status < 400, name, f"{status} {url}")
        except Exception as error:  # noqa: BLE001 - report exact validation failure.
            validator.fail(name, f"unreachable: {url} ({error})")
            return False


def check_published_release_assets(
    validator: ClaimsValidator,
    context: ReleaseContext,
    timeout: float,
) -> None:
    try:
        appcast_data = fetch_url(context.appcast_url, timeout)
        appcast = parse_appcast(appcast_data)
    except Exception as error:  # noqa: BLE001 - report exact validation failure.
        validator.fail("published appcast reachable", f"{context.appcast_url} ({error})")
        return

    validator.pass_("published appcast reachable", context.appcast_url)
    validate_appcast_metadata(validator, appcast, context, "published")

    version = context.expected_version or appcast.version
    build = context.expected_build or appcast.build
    base = context.public_base_url
    urls = [
        ("published immutable Sparkle archive", appcast.enclosure_url),
        ("published latest ZIP", f"{base}/releases/Aetower.zip"),
        ("published latest DMG", f"{base}/releases/Aetower.dmg"),
        ("published latest PKG", f"{base}/releases/Aetower.pkg"),
        ("published Homebrew cask", f"{base}/homebrew/Casks/aetower.rb"),
        ("published third-party notices", f"{base}/third-party-notices.md"),
        ("published latest source archive", f"{base}/releases/Aetower-source.tar.gz"),
        ("published versioned source archive", f"{base}/releases/Aetower-{version}-{build}-source.tar.gz"),
    ]
    for name, url in urls:
        check_url_reachable(validator, name, url, timeout)


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--published", action="store_true", help="validate public release URLs as well")
    parser.add_argument(
        "--require-local-release",
        action="store_true",
        help="fail if dist/Aetower.app or local release artifacts are missing",
    )
    parser.add_argument(
        "--release-env-file",
        default=str(ROOT / ".env.release.local"),
        help="optional release env file to read when variables are not already exported",
    )
    parser.add_argument("--timeout", type=float, default=20.0, help="network timeout for published checks")
    args = parser.parse_args(argv)

    env = load_release_env(Path(args.release_env_file))
    context = release_context(env)
    validator = ClaimsValidator()

    check_history_retention(validator)
    check_mcp_default_tool_safety(validator)
    check_outbound_defaults(validator)
    check_local_release_artifacts(validator, context, args.require_local_release)
    if args.published:
        check_published_release_assets(validator, context, args.timeout)

    validator.print_summary()
    return 1 if validator.failures else 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
