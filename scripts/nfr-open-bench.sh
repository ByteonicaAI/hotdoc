#!/usr/bin/env bash
# NFR-1 (T4): open-time bench — UDP toggle → window visible, p50 ≤ 150ms, p95 ≤ 300ms.
# NFR-1 measures compact-height first paint (660px). The window snap to
# tall (820px) on settings/about open happens later, post-paint, and is
# not measured here.
#
# Drives the app via the UDP toggle mechanism (port TOGGLE_PORT, same as
# `hotdoc-cli toggle`). The window starts visible on first launch, so each
# iteration: hide via xdotool windowunmap → toggle via netcat → poll
# xdotool until visible. Measures the interval in ms.
#
# The WebView is already loaded from first launch (prewarm), so this
# measures the show-path latency, matching what NFR-1 defines as the
# "warm open" gate (spec R1). A cold-start measurement would require
# killing and re-launching for every iteration, which is unreliable in CI.
#
# Usage: scripts/nfr-open-bench.sh <binary_path> [iterations]
# Defaults to 30 iterations (matches the spec NFR-1 sample size used by
# the in-CI bench-open binary; 10 was a single-outlier p95).
# Requires: Xvfb (DISPLAY set), xdotool, netcat (nc).
set -euo pipefail

# ponytail: workspace builds to target/release (not src-tauri/target).
BINARY="${1:-target/release/hotdoc}"
ITERATIONS="${2:-30}"
TOGGLE_PORT=47474
P50_LIMIT_MS=150
P95_LIMIT_MS=300

if [ ! -x "$BINARY" ]; then
  echo "nfr-open-bench: binary '$BINARY' not found" >&2
  exit 1
fi
for tool in xdotool nc; do
  if ! command -v "$tool" &>/dev/null; then
    echo "nfr-open-bench: '$tool' not found — install it" >&2
    exit 1
  fi
done

# Start the app once; keep it alive for all iterations (prewarm scenario).
"$BINARY" &
APP_PID=$!
trap 'kill "$APP_PID" 2>/dev/null || true' EXIT

# Wait for toggle port to be ready (up to 15s).
echo "Waiting for app to start…"
deadline=$((SECONDS + 15))
until nc -zu 127.0.0.1 "$TOGGLE_PORT" 2>/dev/null; do
  if [ $SECONDS -ge $deadline ]; then
    echo "nfr-open-bench: app did not bind toggle port within 15s" >&2
    exit 1
  fi
  sleep 0.2
done

# Wait for initial window to appear.
deadline=$((SECONDS + 10))
until xdotool search --name "hotdoc" &>/dev/null; do
  if [ $SECONDS -ge $deadline ]; then
    echo "nfr-open-bench: window not found within 10s" >&2
    exit 1
  fi
  sleep 0.1
done
sleep 0.5  # let window settle

measurements=()

for i in $(seq 1 "$ITERATIONS"); do
  # Hide the window.
  WIN_ID=$(xdotool search --name "hotdoc" 2>/dev/null | head -1)
  if [ -n "$WIN_ID" ]; then
    xdotool windowunmap "$WIN_ID" 2>/dev/null || true
  fi
  sleep 0.1

  # Measure: toggle → window visible.
  T_START=$(date +%s%3N)
  printf '\x01' | nc -u -w1 127.0.0.1 "$TOGGLE_PORT" &>/dev/null || true

  deadline=$((SECONDS + 2))
  until xdotool search --onlyvisible --name "hotdoc" &>/dev/null; do
    if [ $SECONDS -ge $deadline ]; then
      echo "nfr-open-bench: window not visible after toggle (iteration $i)" >&2
      break
    fi
    sleep 0.005
  done
  T_END=$(date +%s%3N)

  elapsed=$(( T_END - T_START ))
  measurements+=("$elapsed")
  echo "  iter $i: ${elapsed}ms"
  sleep 0.2
done

# Compute p50 and p95.
mapfile -t sorted < <(printf '%s\n' "${measurements[@]}" | sort -n)
n=${#sorted[@]}
p50_idx=$(( (n * 50) / 100 ))
p95_idx=$(( (n * 95) / 100 ))
# Clamp indices.
[ "$p50_idx" -ge "$n" ] && p50_idx=$(( n - 1 ))
[ "$p95_idx" -ge "$n" ] && p95_idx=$(( n - 1 ))
p50=${sorted[$p50_idx]}
p95=${sorted[$p95_idx]}

echo ""
echo "NFR-1 open-time (${n} iterations): p50=${p50}ms, p95=${p95}ms"
echo "  budget: p50≤${P50_LIMIT_MS}ms (hard), p95≤${P95_LIMIT_MS}ms (hard)"

fail=0
if [ "$p50" -gt "$P50_LIMIT_MS" ]; then
  echo "NFR-1 FAIL: p50 ${p50}ms > ${P50_LIMIT_MS}ms"
  fail=1
fi
if [ "$p95" -gt "$P95_LIMIT_MS" ]; then
  echo "NFR-1 FAIL: p95 ${p95}ms > ${P95_LIMIT_MS}ms"
  fail=1
fi
[ "$fail" -eq 0 ] && echo "NFR-1 PASS"
exit "$fail"
