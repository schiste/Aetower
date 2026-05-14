#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP_DIR="$ROOT/dist/Aetower.app"
APP_BIN="$ROOT/dist/Aetower.app/Contents/MacOS/Aetower"
OUT_DIR="${AETOWER_PROFILE_OUT_DIR:-$ROOT/tmp/runtime-profile}"
SUPPORT_DIR="${HOME}/Library/Application Support/Aetower"
DURATION_SECONDS=30
INTERVAL_SECONDS=2
SAMPLE_SECONDS=5
REBUILD=0
LAUNCH=0
ENFORCE=0
SETTLE_SECONDS="${AETOWER_PROFILE_SETTLE_SECONDS:-10}"
CPU_AVG_MAX="${AETOWER_PROFILE_CPU_AVG_MAX:-12.0}"
CPU_MAX_MAX="${AETOWER_PROFILE_CPU_MAX_MAX:-35.0}"
RSS_MAX_KB_MAX="${AETOWER_PROFILE_RSS_MAX_KB_MAX:-524288}"
DIAGNOSTICS_ERROR_MAX="${AETOWER_PROFILE_DIAGNOSTICS_ERROR_MAX:-0}"
CURSOR_UI_MAX="${AETOWER_PROFILE_CURSOR_UI_MAX:-80}"
TCC_REQUEST_MAX="${AETOWER_PROFILE_TCC_REQUEST_MAX:-6}"
DIAGNOSTICS_GROWTH_MAX_BYTES="${AETOWER_PROFILE_DIAGNOSTICS_GROWTH_MAX_BYTES:-262144}"
HISTORY_GROWTH_MAX_BYTES="${AETOWER_PROFILE_HISTORY_GROWTH_MAX_BYTES:-10485760}"
TICK_OVER_BUDGET_MAX="${AETOWER_PROFILE_TICK_OVER_BUDGET_MAX:-0}"
KEEP_RUNS="${AETOWER_PROFILE_KEEP_RUNS:-10}"

while [ "$#" -gt 0 ]; do
    case "$1" in
        --duration)
            DURATION_SECONDS="${2:-}"
            shift 2
            ;;
        --interval)
            INTERVAL_SECONDS="${2:-}"
            shift 2
            ;;
        --sample-seconds)
            SAMPLE_SECONDS="${2:-}"
            shift 2
            ;;
        --rebuild)
            REBUILD=1
            shift
            ;;
        --launch)
            LAUNCH=1
            shift
            ;;
        --enforce)
            ENFORCE=1
            shift
            ;;
        *)
            echo "unsupported argument: $1" >&2
            exit 1
            ;;
    esac
done

if [ "$REBUILD" -eq 1 ] || [ ! -x "$APP_BIN" ]; then
    sh "$ROOT/scripts/package-macos.sh"
fi

mkdir -p "$OUT_DIR"
TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
RUN_DIR="$OUT_DIR/$TIMESTAMP"
mkdir -p "$RUN_DIR"

if [ "$KEEP_RUNS" -gt 0 ]; then
    find "$OUT_DIR" -mindepth 1 -maxdepth 1 -type d 2>/dev/null \
        | grep -E '/[0-9]{8}-[0-9]{6}$' \
        | sort -r \
        | tail -n +$((KEEP_RUNS + 1)) \
        | while IFS= read -r stale; do
            rm -rf "$stale"
        done
fi
METRICS_CSV="$RUN_DIR/metrics.csv"
SAMPLE_TXT="$RUN_DIR/sample.txt"
SUMMARY_TXT="$RUN_DIR/summary.txt"
STORE_TXT="$RUN_DIR/store-summary.txt"
SESSION_LOG_TXT="$RUN_DIR/session-log-summary.txt"

APP_PID=""
cleanup() {
    if [ -n "$APP_PID" ] && [ "$LAUNCH" -eq 1 ]; then
        kill "$APP_PID" 2>/dev/null || true
        wait "$APP_PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT INT TERM

if [ "$LAUNCH" -eq 1 ]; then
    open "$APP_DIR"
    ATTEMPTS=0
    while [ "$ATTEMPTS" -lt 20 ]; do
        APP_PID="$(ps -axo pid=,command= | awk '/Aetower\.app\/Contents\/MacOS\/Aetower/ && !/awk/ {print $1; exit}')"
        if [ -n "$APP_PID" ]; then
            break
        fi
        ATTEMPTS=$((ATTEMPTS + 1))
        sleep 1
    done
    if [ -z "$APP_PID" ]; then
        echo "failed to launch packaged Aetower app" >&2
        exit 1
    fi
else
    APP_PID="$(ps -axo pid=,command= | awk '/Aetower\.app\/Contents\/MacOS\/Aetower/ && !/awk/ {print $1; exit}')"
    if [ -z "$APP_PID" ]; then
        echo "no running packaged Aetower process found; use --launch or start the app first" >&2
        exit 1
    fi
fi

file_bytes() {
    if [ -f "$1" ]; then
        wc -c <"$1" | tr -d ' '
    else
        printf '0'
    fi
}

count_matches() {
    if [ -f "$1" ]; then
        grep -c "$2" "$1" || true
    else
        printf '0'
    fi
}

DIAGNOSTICS_FILE="$SUPPORT_DIR/diagnostics.ndjson"
HISTORY_DB="$SUPPORT_DIR/history.db"
if [ "$SETTLE_SECONDS" -gt 0 ]; then
    sleep "$SETTLE_SECONDS"
fi
START_DIAGNOSTICS_BYTES="$(file_bytes "$DIAGNOSTICS_FILE")"
START_HISTORY_BYTES="$(file_bytes "$HISTORY_DB")"
START_TICK_OVER_BUDGET="$(count_matches "$DIAGNOSTICS_FILE" '"event_type":"tick-over-budget"')"
START_DIAGNOSTICS_ERROR="$(count_matches "$DIAGNOSTICS_FILE" '"level":"error"')"

printf 'timestamp,pid,cpu_percent,rss_kb,vsz_kb\n' >"$METRICS_CSV"

END_EPOCH="$(($(date +%s) + DURATION_SECONDS))"
while [ "$(date +%s)" -lt "$END_EPOCH" ]; do
    ps -p "$APP_PID" -o pid=,%cpu=,rss=,vsz= | awk -v now="$(date +%s)" 'NF == 4 {printf "%s,%s,%s,%s,%s\n", now, $1, $2, $3, $4}'
    sleep "$INTERVAL_SECONDS"
done >>"$METRICS_CSV"

if command -v sample >/dev/null 2>&1; then
    sample "$APP_PID" "$SAMPLE_SECONDS" -file "$SAMPLE_TXT" >/dev/null 2>&1 || true
fi

awk -F, '
NR == 1 { next }
{
    cpu_sum += $3
    rss_sum += $4
    count += 1
    if ($3 > cpu_max) cpu_max = $3
    if ($4 > rss_max) rss_max = $4
}
END {
    if (count == 0) {
        print "No samples captured"
        exit 1
    }
    printf "pid: %s\n", pid
    printf "samples: %d\n", count
    printf "cpu avg: %.2f%%\n", cpu_sum / count
    printf "cpu max: %.2f%%\n", cpu_max
    printf "rss avg: %.0f KB\n", rss_sum / count
    printf "rss max: %.0f KB\n", rss_max
}' pid="$APP_PID" "$METRICS_CSV" >"$SUMMARY_TXT"

DIAGNOSTICS_LINES=0
DIAGNOSTICS_WARN=0
DIAGNOSTICS_ERROR=0
DIAGNOSTICS_BYTES=0
if [ -f "$DIAGNOSTICS_FILE" ]; then
    DIAGNOSTICS_LINES="$(wc -l <"$DIAGNOSTICS_FILE" | tr -d ' ')"
    DIAGNOSTICS_BYTES="$(wc -c <"$DIAGNOSTICS_FILE" | tr -d ' ')"
    DIAGNOSTICS_WARN="$(grep -c '"level":"warn"' "$DIAGNOSTICS_FILE" || true)"
    DIAGNOSTICS_ERROR="$(grep -c '"level":"error"' "$DIAGNOSTICS_FILE" || true)"
fi
HISTORY_BYTES=0
if [ -f "$HISTORY_DB" ]; then
    HISTORY_BYTES="$(wc -c <"$HISTORY_DB" | tr -d ' ')"
fi
DIAGNOSTICS_GROWTH_BYTES="$(awk -v end="$DIAGNOSTICS_BYTES" -v start="$START_DIAGNOSTICS_BYTES" 'BEGIN { print end - start }')"
HISTORY_GROWTH_BYTES="$(awk -v end="$HISTORY_BYTES" -v start="$START_HISTORY_BYTES" 'BEGIN { print end - start }')"
END_TICK_OVER_BUDGET="$(count_matches "$DIAGNOSTICS_FILE" '"event_type":"tick-over-budget"')"
TICK_OVER_BUDGET_DELTA="$(awk -v end="$END_TICK_OVER_BUDGET" -v start="$START_TICK_OVER_BUDGET" 'BEGIN { print end - start }')"
DIAGNOSTICS_ERROR_DELTA="$(awk -v end="$DIAGNOSTICS_ERROR" -v start="$START_DIAGNOSTICS_ERROR" 'BEGIN { print end - start }')"

{
    printf 'diagnostics file: %s\n' "$DIAGNOSTICS_FILE"
    printf 'diagnostics start bytes: %s\n' "$START_DIAGNOSTICS_BYTES"
    printf 'diagnostics bytes: %s\n' "$DIAGNOSTICS_BYTES"
    printf 'diagnostics growth bytes: %s\n' "$DIAGNOSTICS_GROWTH_BYTES"
    printf 'diagnostics lines: %s\n' "$DIAGNOSTICS_LINES"
    printf 'diagnostics warn entries: %s\n' "$DIAGNOSTICS_WARN"
    printf 'diagnostics error entries: %s\n' "$DIAGNOSTICS_ERROR"
    printf 'diagnostics error delta: %s\n' "$DIAGNOSTICS_ERROR_DELTA"
    printf 'tick over budget delta: %s\n' "$TICK_OVER_BUDGET_DELTA"
    printf 'history db: %s\n' "$HISTORY_DB"
    printf 'history start bytes: %s\n' "$START_HISTORY_BYTES"
    printf 'history db bytes: %s\n' "$HISTORY_BYTES"
    printf 'history growth bytes: %s\n' "$HISTORY_GROWTH_BYTES"
} >"$STORE_TXT"

if command -v /usr/bin/log >/dev/null 2>&1; then
    /usr/bin/log show --style compact --last "${DURATION_SECONDS}s" --predicate "processIdentifier == $APP_PID" 2>/dev/null | awk '
        /TextInputUI.*CursorUI/ { cursor += 1 }
        /TCCAccessRequest\(\) IPC/ { tcc += 1 }
        /Unable to open mach-O at path/ { metal += 1 }
        /NSViewBridgeErrorCanceled/ { bridge += 1 }
        /ordered front from a non-active application/ { window += 1 }
        END {
            printf "cursor_ui_entries: %d\n", cursor + 0
            printf "tcc_access_requests: %d\n", tcc + 0
            printf "metal_load_failures: %d\n", metal + 0
            printf "view_bridge_cancellations: %d\n", bridge + 0
            printf "non_active_window_warnings: %d\n", window + 0
        }
    ' >"$SESSION_LOG_TXT" || true
fi

cat "$SUMMARY_TXT"
printf 'artifacts: %s\n' "$RUN_DIR"

if [ "$ENFORCE" -eq 1 ]; then
    CPU_AVG="$(awk -F': ' '/^cpu avg:/ {gsub(/%/, "", $2); print $2}' "$SUMMARY_TXT")"
    CPU_MAX="$(awk -F': ' '/^cpu max:/ {gsub(/%/, "", $2); print $2}' "$SUMMARY_TXT")"
    RSS_MAX="$(awk -F': ' '/^rss max:/ {gsub(/ KB/, "", $2); print $2}' "$SUMMARY_TXT")"
    DIAGNOSTICS_ERRORS="$(awk -F': ' '/^diagnostics error delta:/ {print $2}' "$STORE_TXT")"
    DIAGNOSTICS_GROWTH="$(awk -F': ' '/^diagnostics growth bytes:/ {print $2}' "$STORE_TXT")"
    HISTORY_GROWTH="$(awk -F': ' '/^history growth bytes:/ {print $2}' "$STORE_TXT")"
    TICK_OVER_BUDGET_DELTA="$(awk -F': ' '/^tick over budget delta:/ {print $2}' "$STORE_TXT")"
    CURSOR_UI="$(awk -F': ' '/^cursor_ui_entries:/ {print $2}' "$SESSION_LOG_TXT" 2>/dev/null || printf '0')"
    TCC_REQUESTS="$(awk -F': ' '/^tcc_access_requests:/ {print $2}' "$SESSION_LOG_TXT" 2>/dev/null || printf '0')"

    awk -v value="$CPU_AVG" -v limit="$CPU_AVG_MAX" 'BEGIN { exit !(value <= limit) }' || {
        echo "runtime profile enforcement failed: cpu avg $CPU_AVG > $CPU_AVG_MAX" >&2
        exit 1
    }
    awk -v value="$CPU_MAX" -v limit="$CPU_MAX_MAX" 'BEGIN { exit !(value <= limit) }' || {
        echo "runtime profile enforcement failed: cpu max $CPU_MAX > $CPU_MAX_MAX" >&2
        exit 1
    }
    awk -v value="$RSS_MAX" -v limit="$RSS_MAX_KB_MAX" 'BEGIN { exit !(value <= limit) }' || {
        echo "runtime profile enforcement failed: rss max $RSS_MAX KB > $RSS_MAX_KB_MAX KB" >&2
        exit 1
    }
    awk -v value="$DIAGNOSTICS_ERRORS" -v limit="$DIAGNOSTICS_ERROR_MAX" 'BEGIN { exit !(value <= limit) }' || {
        echo "runtime profile enforcement failed: diagnostics errors $DIAGNOSTICS_ERRORS > $DIAGNOSTICS_ERROR_MAX" >&2
        exit 1
    }
    awk -v value="$DIAGNOSTICS_GROWTH" -v limit="$DIAGNOSTICS_GROWTH_MAX_BYTES" 'BEGIN { exit !(value <= limit) }' || {
        echo "runtime profile enforcement failed: diagnostics growth $DIAGNOSTICS_GROWTH bytes > $DIAGNOSTICS_GROWTH_MAX_BYTES bytes" >&2
        exit 1
    }
    awk -v value="$HISTORY_GROWTH" -v limit="$HISTORY_GROWTH_MAX_BYTES" 'BEGIN { exit !(value <= limit) }' || {
        echo "runtime profile enforcement failed: history growth $HISTORY_GROWTH bytes > $HISTORY_GROWTH_MAX_BYTES bytes" >&2
        exit 1
    }
    awk -v value="$TICK_OVER_BUDGET_DELTA" -v limit="$TICK_OVER_BUDGET_MAX" 'BEGIN { exit !(value <= limit) }' || {
        echo "runtime profile enforcement failed: tick-over-budget delta $TICK_OVER_BUDGET_DELTA > $TICK_OVER_BUDGET_MAX" >&2
        exit 1
    }
    awk -v value="$CURSOR_UI" -v limit="$CURSOR_UI_MAX" 'BEGIN { exit !(value <= limit) }' || {
        echo "runtime profile enforcement failed: CursorUI noise $CURSOR_UI > $CURSOR_UI_MAX" >&2
        exit 1
    }
    awk -v value="$TCC_REQUESTS" -v limit="$TCC_REQUEST_MAX" 'BEGIN { exit !(value <= limit) }' || {
        echo "runtime profile enforcement failed: TCC requests $TCC_REQUESTS > $TCC_REQUEST_MAX" >&2
        exit 1
    }
    printf '✓ runtime profile enforcement passed\n'
fi
