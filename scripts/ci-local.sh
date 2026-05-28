#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

if [ "${AETOWER_CI_LOGGING_ACTIVE:-0}" != "1" ]; then
    requested_mode="pre-commit"
    previous_arg=""
    for arg in "$@"; do
        if [ "$arg" = "--full" ]; then
            requested_mode="full"
            previous_arg=""
            continue
        fi
        if [ "$previous_arg" = "--mode" ]; then
            requested_mode="$arg"
            previous_arg=""
            continue
        fi
        previous_arg="$arg"
    done
    case "$requested_mode" in
        prepush) requested_mode="pre-push" ;;
        prepush-full|local|cloud-parity) requested_mode="full" ;;
    esac
    if [ "$requested_mode" != "pre-commit" ]; then
        mkdir -p "$ROOT/logs"
        log_file="$ROOT/logs/quality-$requested_mode-$(date +%Y%m%d-%H%M%S).log"
        status_file="$(mktemp "${TMPDIR:-/tmp}/aetower-ci-status.XXXXXX")"
        (
            set +e
            AETOWER_CI_LOGGING_ACTIVE=1 AETOWER_CI_LOG_FILE="$log_file" sh "$0" "$@"
            inner_status="$?"
            printf '%s' "$inner_status" > "$status_file"
            exit "$inner_status"
        ) 2>&1 | tee "$log_file"
        status="$(cat "$status_file" 2>/dev/null || printf '1')"
        rm -f "$status_file"
        exit "$status"
    fi
fi

MODE="pre-commit"
BENCH_ITERATIONS=""
INCLUDE_FILTER=""
SKIP_FILTER=""
PUSH_STDIN_FILE=""
SWIFTLINT_CACHE_DIR="$ROOT/tmp/swiftlint-cache"
SWIFT_BUILD_DIR="$ROOT/macos/.build"
CLANG_MODULE_CACHE_PATH="${CLANG_MODULE_CACHE_PATH:-$ROOT/tmp/clang-module-cache}"
SUMMARY_FILE=""
SUMMARY_PRINTED=0
RUN_STARTED_AT="$(date +%s)"
export CLANG_MODULE_CACHE_PATH

while [ "$#" -gt 0 ]; do
    case "$1" in
        --mode)
            MODE="${2:-}"
            shift 2
            ;;
        --iterations)
            BENCH_ITERATIONS="${2:-}"
            shift 2
            ;;
        --include)
            INCLUDE_FILTER="${2:-}"
            shift 2
            ;;
        --skip)
            SKIP_FILTER="${2:-}"
            shift 2
            ;;
        --push-stdin-file)
            PUSH_STDIN_FILE="${2:-}"
            shift 2
            ;;
        --full)
            MODE="full"
            shift
            ;;
        *)
            echo "unsupported argument: $1" >&2
            exit 1
            ;;
    esac
done

case "$MODE" in
    prepush) MODE="pre-push" ;;
    prepush-full|local|cloud-parity) MODE="full" ;;
esac

format_duration() {
    seconds="$1"
    if [ "$seconds" -lt 1 ]; then
        printf '<1s'
    elif [ "$seconds" -lt 60 ]; then
        printf '%ss' "$seconds"
    else
        minutes=$((seconds / 60))
        remainder=$((seconds % 60))
        printf '%sm %02ss' "$minutes" "$remainder"
    fi
}

record_step() {
    step_label="$1"
    step_status="$2"
    step_duration="$3"
    step_detail="${4:-}"
    if [ -n "$SUMMARY_FILE" ]; then
        printf '%s\t%s\t%s\t%s\n' "$step_label" "$step_status" "$step_duration" "$step_detail" >> "$SUMMARY_FILE"
    fi
}

format_command() {
    first=1
    for arg in "$@"; do
        if [ "$first" -eq 1 ]; then
            first=0
        else
            printf ' '
        fi
        printf '%s' "$arg"
    done
}

filter_contains() {
    filter="$1"
    label="$2"
    if [ -z "$filter" ]; then
        return 1
    fi
    old_ifs="$IFS"
    IFS=","
    for item in $filter; do
        IFS="$old_ifs"
        if [ "$item" = "$label" ]; then
            return 0
        fi
        IFS=","
    done
    IFS="$old_ifs"
    return 1
}

should_run_label() {
    label="$1"
    if [ -n "$INCLUDE_FILTER" ] && ! filter_contains "$INCLUDE_FILTER" "$label"; then
        skip "$label" "excluded by --include=$INCLUDE_FILTER"
        return 1
    fi
    if filter_contains "$SKIP_FILTER" "$label"; then
        skip "$label" "excluded by --skip=$SKIP_FILTER"
        return 1
    fi
    return 0
}

run() {
    label="$1"
    shift
    if ! should_run_label "$label"; then
        return
    fi
    command_text="$(format_command "$@")"
    printf '\n==> %s\n' "$label"
    printf '    command: %s\n' "$command_text"
    start="$(date +%s)"
    set +e
    "$@"
    status="$?"
    set -e
    duration=$(( $(date +%s) - start ))
    if [ "$status" -eq 0 ]; then
        record_step "$label" "PASS" "$duration"
        printf '    result: PASS (%s)\n' "$(format_duration "$duration")"
    else
        record_step "$label" "FAIL" "$duration" "exit $status; rerun: $command_text"
        printf '    result: FAIL (%s, exit %s)\n' "$(format_duration "$duration")" "$status" >&2
        return "$status"
    fi
}

skip() {
    label="$1"
    reason="$2"
    printf '\n--> skip: %s\n' "$label"
    printf '    reason: %s\n' "$reason"
    record_step "$label" "SKIP" 0 "$reason"
}

print_summary() {
    exit_status="$1"
    if [ "$SUMMARY_PRINTED" -eq 1 ]; then
        return
    fi
    SUMMARY_PRINTED=1

    total_seconds=$(( $(date +%s) - RUN_STARTED_AT ))
    passed=0
    skipped=0
    failed=0
    ran=0
    measured_seconds=0

    if [ -n "$SUMMARY_FILE" ] && [ -f "$SUMMARY_FILE" ]; then
        while IFS='	' read -r step_label step_status step_duration step_detail; do
            case "$step_status" in
                PASS)
                    passed=$((passed + 1))
                    ran=$((ran + 1))
                    measured_seconds=$((measured_seconds + step_duration))
                    ;;
                FAIL)
                    failed=$((failed + 1))
                    ran=$((ran + 1))
                    measured_seconds=$((measured_seconds + step_duration))
                    ;;
                SKIP)
                    skipped=$((skipped + 1))
                    ;;
            esac
        done < "$SUMMARY_FILE"
    fi

    printf '\n============================================================\n'
    printf 'Quality run summary (%s)\n' "$MODE"
    printf '============================================================\n'

    if [ -n "$SUMMARY_FILE" ] && [ -f "$SUMMARY_FILE" ]; then
        while IFS='	' read -r step_label step_status step_duration step_detail; do
            duration_text="$(format_duration "$step_duration")"
            percent="0.0"
            if [ "$measured_seconds" -gt 0 ] && [ "$step_status" != "SKIP" ]; then
                percent="$(awk -v duration="$step_duration" -v total="$measured_seconds" 'BEGIN { printf "%.1f", (duration / total) * 100 }')"
            fi
            case "$step_status" in
                PASS) marker="[PASS]" ;;
                FAIL) marker="[FAIL]" ;;
                SKIP) marker="[SKIP]" ;;
                *) marker="[$step_status]" ;;
            esac
            printf '%-6s %-50s | %8s | %5s%%' "$marker" "$step_label" "$duration_text" "$percent"
            if [ "$step_status" = "SKIP" ] && [ -n "$step_detail" ]; then
                printf ' | %s' "$step_detail"
            fi
            if [ "$step_status" = "FAIL" ] && [ -n "$step_detail" ]; then
                printf ' | %s' "$step_detail"
            fi
            printf '\n'
        done < "$SUMMARY_FILE"
    fi

    printf '%s\n' '------------------------------------------------------------'
    if [ "$exit_status" -eq 0 ]; then
        printf '[PASS] Quality run complete in %s\n' "$(format_duration "$total_seconds")"
    else
        printf '[FAIL] Quality run failed in %s\n' "$(format_duration "$total_seconds")"
    fi
    printf 'total %s | ran %s | passed %s | skipped %s | failed %s\n' \
        "$(format_duration "$total_seconds")" "$ran" "$passed" "$skipped" "$failed"
    if [ -n "${AETOWER_CI_LOG_FILE:-}" ]; then
        printf 'log %s\n' "$AETOWER_CI_LOG_FILE"
    fi
    printf '============================================================\n'

    if [ -n "$SUMMARY_FILE" ] && [ -f "$SUMMARY_FILE" ]; then
        rm -f "$SUMMARY_FILE"
    fi
}

resolve_cargo_bin() {
    cargo_bin="$(command -v cargo || true)"
    if [ -n "${HOME:-}" ] \
        && [ -x "$HOME/.cargo/bin/cargo" ] \
        && [ "$cargo_bin" = "$HOME/.chau7/cto_bin/cargo" ]; then
        printf '%s\n' "$HOME/.cargo/bin/cargo"
        return
    fi
    printf '%s\n' "${cargo_bin:-cargo}"
}

require_tool() {
    tool="$1"
    install_hint="$2"
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "missing required tool: $tool ($install_hint)" >&2
        exit 1
    fi
}

semgrep_healthy() {
    if ! command -v semgrep >/dev/null 2>&1; then
        return 1
    fi
    env SEMGREP_SEND_METRICS=off semgrep --help >/dev/null 2>&1
}

remove_tree() {
    target_path="$1"
    attempts="${2:-5}"
    sleep_seconds="${3:-1}"

    if [ ! -e "$target_path" ]; then
        return 0
    fi

    attempt=1
    while [ "$attempt" -le "$attempts" ]; do
        rm -rf "$target_path" 2>/dev/null || true
        if [ ! -e "$target_path" ]; then
            return 0
        fi
        if [ "$attempt" -lt "$attempts" ]; then
            sleep "$sleep_seconds"
        fi
        attempt=$((attempt + 1))
    done

    echo "failed to remove $target_path after $attempts attempt(s)" >&2
    return 1
}

clean_swift_build_dir() {
    remove_tree "$SWIFT_BUILD_DIR"
}

staged_files() {
    git diff --cached --name-only --diff-filter=ACMR
}

has_staged_match() {
    pattern="$1"
    staged_files | rg -q "$pattern"
}

affected_rust_packages() {
    staged_files \
        | awk -F/ '$1 == "rust" && $2 == "crates" && $3 != "" { print $3 }' \
        | sort -u
}

changed_swift_files() {
    staged_files \
        | rg '^macos/.*\.swift$' \
        | rg -v '^(macos/Sources/AetowerBindings/|macos/Sources/aetower_ffiFFI/)' \
        || true
}

changed_semgrep_files() {
    staged_files \
        | rg '^(macos/Sources/Aetower(UI|App|Bridge)/.*\.swift|rust/crates/.+\.rs|scripts/.+\.sh|\.githooks/.+)$' \
        | rg -v '^(macos/Sources/AetowerBindings/|macos/Sources/aetower_ffiFFI/)' \
        || true
}

fallback_push_base() {
    if git rev-parse --verify '@{upstream}' >/dev/null 2>&1; then
        printf '%s' '@{upstream}'
    elif git rev-parse --verify 'origin/main' >/dev/null 2>&1; then
        printf '%s' 'origin/main'
    elif git rev-parse --verify 'main' >/dev/null 2>&1; then
        printf '%s' 'main'
    elif git rev-parse --verify 'HEAD~1' >/dev/null 2>&1; then
        printf '%s' 'HEAD~1'
    else
        printf '%s' 'HEAD'
    fi
}

push_diff_base() {
    if [ -n "$PUSH_STDIN_FILE" ] && [ -s "$PUSH_STDIN_FILE" ]; then
        while read -r local_ref local_sha remote_ref remote_sha; do
            if [ -z "${local_sha:-}" ]; then
                continue
            fi
            case "$remote_sha" in
                0000000000000000000000000000000000000000)
                    fallback_push_base
                    return
                    ;;
                *)
                    printf '%s' "$remote_sha"
                    return
                    ;;
            esac
        done < "$PUSH_STDIN_FILE"
    fi
    fallback_push_base
}

confirm_dirty_worktree_for_push() {
    if [ "$MODE" != "pre-push" ] && [ "$MODE" != "full" ]; then
        return
    fi
    if git diff --quiet && git diff --cached --quiet; then
        return
    fi
    if [ "${AETOWER_SKIP_DIRTY_WORKTREE_CONFIRM:-0}" = "1" ]; then
        printf 'dirty worktree confirmation skipped by AETOWER_SKIP_DIRTY_WORKTREE_CONFIRM=1\n'
        return
    fi
    if tty_path="$(tty 2>/dev/null)" && [ -n "$tty_path" ] && [ "$tty_path" != "not a tty" ]; then
        printf 'The worktree is dirty. Continue with the quality run? [y/N] ' > "$tty_path"
        read -r answer < "$tty_path"
        case "$answer" in
            y|Y|yes|YES) return ;;
        esac
        echo "aborted because the worktree is dirty" >&2
        exit 1
    fi
    echo "dirty worktree detected; set AETOWER_SKIP_DIRTY_WORKTREE_CONFIRM=1 to run non-interactively" >&2
    exit 1
}

should_run_full_rust_gate() {
    if has_staged_match '^(rust/Cargo\.toml|rust/Cargo\.lock|rustfmt\.toml|rust-toolchain\.toml|scripts/build-rust\.sh|scripts/build-rust-ffi-for-swiftpm\.sh)$'; then
        return 0
    fi
    return 1
}

run_precommit_rust() {
    if ! has_staged_match '^rust/|^rustfmt\.toml$|^rust-toolchain\.toml$'; then
        skip "Rust staged gate" "no staged Rust changes"
        return
    fi

    run "cargo fmt --check" "$CARGO_BIN" fmt --manifest-path "$ROOT/rust/Cargo.toml" --all -- --check

    if should_run_full_rust_gate; then
        run "cargo clippy (workspace)" "$CARGO_BIN" clippy --locked --manifest-path "$ROOT/rust/Cargo.toml" --all-targets -- -D warnings
        run_workspace_tests
        return
    fi

    packages="$(affected_rust_packages)"
    if [ -z "$packages" ]; then
        skip "Rust package gate" "no affected Rust package detected"
        return
    fi

    for pkg in $packages; do
        run "cargo clippy ($pkg)" "$CARGO_BIN" clippy --locked --manifest-path "$ROOT/rust/Cargo.toml" -p "$pkg" --all-targets -- -D warnings
        run "cargo test ($pkg)" "$CARGO_BIN" test --locked --manifest-path "$ROOT/rust/Cargo.toml" -p "$pkg"
    done
}

run_precommit_gitleaks() {
    require_tool "gitleaks" "install with: brew install gitleaks"
    run "gitleaks (staged)" gitleaks git --pre-commit --staged --no-banner --redact
}

run_precommit_swiftlint() {
    files="$(changed_swift_files)"
    if [ -z "$files" ]; then
        skip "swiftlint (staged Swift)" "no staged Swift source changes"
        return
    fi
    require_tool "swiftlint" "install with: brew install swiftlint"
    mkdir -p "$SWIFTLINT_CACHE_DIR"
    for path in $files; do
        run "swiftlint ($path)" swiftlint lint --strict --quiet --force-exclude --cache-path "$SWIFTLINT_CACHE_DIR" --config "$ROOT/.swiftlint.yml" "$ROOT/$path"
    done
}

run_precommit_semgrep() {
    files="$(changed_semgrep_files)"
    if [ -z "$files" ]; then
        skip "semgrep (changed files)" "no staged files matched local Semgrep scope"
        return
    fi
    if ! semgrep_healthy; then
        skip "semgrep (changed files)" "local binary unavailable or unhealthy"
        return
    fi
    run "semgrep (changed files)" env SEMGREP_SEND_METRICS=off semgrep scan --metrics=off --error --quiet --config "$ROOT/.semgrep/local-quality.yml" $files
}

run_workspace_tests() {
    run "cargo test (workspace)" "$CARGO_BIN" test --locked --manifest-path "$ROOT/rust/Cargo.toml" --workspace --exclude aetower-helper
    run "cargo test (aetower-helper)" "$CARGO_BIN" test --locked --manifest-path "$ROOT/rust/Cargo.toml" -p aetower-helper -- --test-threads=1
}

run_precommit_swift() {
    if ! has_staged_match '^macos/.*\.swift$|^macos/Package\.swift$'; then
        skip "swift build" "no staged Swift package changes"
        return
    fi
    run "swift build" /usr/bin/swift build --package-path "$ROOT/macos" --scratch-path "$SWIFT_BUILD_DIR"
}

run_precommit_bridge() {
    if ! has_staged_match '^rust/crates/aetower-ffi/|^macos/Sources/AetowerBindings/aetower_ffi\.swift$|^macos/Sources/aetower_ffiFFI/aetower_ffiFFI\.h$|^macos/Sources/AetowerBridge/|^scripts/build-rust'; then
        skip "build Rust bridge" "no staged FFI or bridge changes"
        return
    fi
    clean_swift_build_dir
    run "build Rust bridge" sh "$ROOT/scripts/build-rust.sh"
}

run_precommit_benchmark() {
    if ! has_staged_match '^rust/crates/aetower-(core|collector|history|persistence|telemetry|gpu|model|diagnostics|friction|attribution|identity)/'; then
        skip "benchmark smoke" "no staged runtime crate changes"
        return
    fi
    iterations="${BENCH_ITERATIONS:-4}"
    run "benchmark smoke" sh "$ROOT/scripts/measure-overhead.sh" --iterations "$iterations" --enforce
}

run_precommit_shell() {
    if ! has_staged_match '(^scripts/.*\.sh$|^\.githooks/)'; then
        skip "shell hook checks" "no staged shell or hook changes"
        return
    fi
    skip "shell hook checks" "no shell-specific checker configured"
}

run_precommit() {
    run "quality guard" python3 "$ROOT/scripts/quality-guard.py" --mode pre-commit
    run_precommit_gitleaks
    run_precommit_swiftlint
    run_precommit_semgrep
    run_precommit_rust
    run_precommit_bridge
    run_precommit_swift
    run_precommit_benchmark
    run_precommit_shell
}

run_full_gitleaks() {
    require_tool "gitleaks" "install with: brew install gitleaks"
    base="$(push_diff_base)"
    run "gitleaks (commits)" gitleaks git --no-banner --redact --log-opts "${base}..HEAD"
}

run_full_swiftlint() {
    require_tool "swiftlint" "install with: brew install swiftlint"
    mkdir -p "$SWIFTLINT_CACHE_DIR"
    run "swiftlint" swiftlint lint --strict --quiet --force-exclude --cache-path "$SWIFTLINT_CACHE_DIR" --config "$ROOT/.swiftlint.yml" "$ROOT/macos/Sources"
}

run_full_semgrep() {
    if ! should_run_label "semgrep"; then
        return
    fi
    if ! semgrep_healthy; then
        skip "semgrep" "local binary unavailable or unhealthy"
        return
    fi
    run "semgrep" env SEMGREP_SEND_METRICS=off semgrep scan --metrics=off --error --quiet --config "$ROOT/.semgrep/local-quality.yml" "$ROOT/macos/Sources/AetowerUI" "$ROOT/macos/Sources/AetowerApp" "$ROOT/macos/Sources/AetowerBridge" "$ROOT/rust/crates" "$ROOT/scripts"
}

run_full_dependency_policy() {
    deny_bin="$(command -v cargo-deny || true)"
    if [ -z "$deny_bin" ]; then
        echo "missing required tool: cargo-deny (install with: cargo install cargo-deny --locked)" >&2
        exit 1
    fi
    run "cargo deny" sh -c 'cd "$1/rust" && CARGO="$2" "$3" check advisories bans sources --disable-fetch --config "$1/deny.toml"' sh "$ROOT" "$CARGO_BIN" "$deny_bin"
}

run_full_gate() {
    confirm_dirty_worktree_for_push
    mkdir -p "$CLANG_MODULE_CACHE_PATH"

    if [ -z "$BENCH_ITERATIONS" ]; then
        case "$MODE" in
            pre-push) BENCH_ITERATIONS="20" ;;
            full) BENCH_ITERATIONS="30" ;;
            *) BENCH_ITERATIONS="20" ;;
        esac
    fi

    if ! git diff --quiet || ! git diff --cached --quiet; then
        run "quality guard (working tree)" python3 "$ROOT/scripts/quality-guard.py" --mode working-tree
    fi
    run "quality guard" python3 "$ROOT/scripts/quality-guard.py" --mode "$MODE"
    run_full_gitleaks
    run_full_swiftlint
    run_full_semgrep
    run "cargo fmt --check" "$CARGO_BIN" fmt --manifest-path "$ROOT/rust/Cargo.toml" --all -- --check
    run "cargo clippy" "$CARGO_BIN" clippy --locked --manifest-path "$ROOT/rust/Cargo.toml" --all-targets -- -D warnings
    run_workspace_tests
    clean_swift_build_dir
    run "build Rust bridge" sh "$ROOT/scripts/build-rust.sh"
    run "swift build" /usr/bin/swift build --package-path "$ROOT/macos" --scratch-path "$SWIFT_BUILD_DIR"
    run "benchmark budget" sh "$ROOT/scripts/measure-overhead.sh" --iterations "$BENCH_ITERATIONS" --enforce
    run "telemetry smoke" sh "$ROOT/scripts/telemetry-smoke.sh"
    run "package smoke" sh "$ROOT/scripts/quality-package-smoke.sh"
    if [ "$MODE" = "full" ]; then
        run "local operator smoke" sh "$ROOT/scripts/local-operator-smoke.sh"
    fi
    run_full_dependency_policy
    if [ "$MODE" = "full" ]; then
        if command -v cargo-audit >/dev/null 2>&1; then
            run "cargo audit" cargo audit --file "$ROOT/rust/Cargo.lock" --ignore RUSTSEC-2025-0141 --ignore RUSTSEC-2026-0097
        else
            skip "cargo audit" "cargo-audit is not installed"
        fi
    fi
}

cd "$ROOT"
CARGO_BIN="$(resolve_cargo_bin)"
SUMMARY_FILE="$(mktemp "${TMPDIR:-/tmp}/aetower-ci-summary.XXXXXX")"
trap 'status=$?; print_summary "$status"' EXIT
trap 'status=$?; print_summary "$status"; exit "$status"' INT TERM

case "$MODE" in
    pre-commit)
        run_precommit
        ;;
    pre-push|full)
        run_full_gate
        ;;
    *)
        echo "unsupported mode: $MODE" >&2
        exit 1
        ;;
esac

printf '\n✓ local ci passed (%s)\n' "$MODE"
