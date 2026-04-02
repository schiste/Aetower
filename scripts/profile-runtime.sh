#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP_BIN="$ROOT/dist/Aetower.app/Contents/MacOS/Aetower"
OUT_DIR="${AETOWER_PROFILE_OUT_DIR:-$ROOT/tmp/runtime-profile}"
DURATION_SECONDS=30
INTERVAL_SECONDS=2
SAMPLE_SECONDS=5
REBUILD=0
LAUNCH=0

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
METRICS_CSV="$RUN_DIR/metrics.csv"
SAMPLE_TXT="$RUN_DIR/sample.txt"
SUMMARY_TXT="$RUN_DIR/summary.txt"

APP_PID=""
cleanup() {
    if [ -n "$APP_PID" ] && [ "$LAUNCH" -eq 1 ]; then
        kill "$APP_PID" 2>/dev/null || true
        wait "$APP_PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT INT TERM

if [ "$LAUNCH" -eq 1 ]; then
    "$APP_BIN" >/dev/null 2>&1 &
    APP_PID=$!
    sleep 3
else
    APP_PID="$(ps -axo pid=,command= | awk '/Aetower\.app\/Contents\/MacOS\/Aetower/ && !/awk/ {print $1; exit}')"
    if [ -z "$APP_PID" ]; then
        echo "no running packaged Aetower process found; use --launch or start the app first" >&2
        exit 1
    fi
fi

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

cat "$SUMMARY_TXT"
printf 'artifacts: %s\n' "$RUN_DIR"
