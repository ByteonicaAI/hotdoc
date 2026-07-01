#!/usr/bin/env bash
# NFR-1 (T4): open-time bench — UDP toggle → window visible, p50 ≤ 150ms, p95 ≤ 300ms.
# NFR-1 measures compact-height first paint (660px). The window snap to
# tall (820px) on settings/about open happens later, post-paint, and is
# not measured here.
#
# REAL-DISPLAY GATE. This bench needs a display that can actually map and
# paint the transparent `main` window (GPU or a compositor). A headless CI
# runner (Xvfb, software llvmpipe, no compositor) never realizes the window,
# so this cannot run there — it runs on a developer machine / release QA via
# `verify:all`. Headless CI instead runs scripts/nfr-open-liveness.sh, which
# verifies the toggle path is wired without timing paint.
#
# Drives the app via the UDP toggle mechanism (port TOGGLE_PORT, same as
# `hotdoc-cli toggle`). The window is created `visible: false`, so we send an
# initial toggle to realize + show it, then each iteration: hide via xdotool
# windowunmap → toggle via netcat → poll xdotool until visible. Measures the
# interval in ms.
#
# The WebView is already loaded from first launch (prewarm), so this
# measures the show-path latency, matching what NFR-1 defines as the
# "warm open" gate (spec R1). A cold-start measurement would require
# killing and re-launching for every iteration, which is unreliable in CI.
#
# Usage: scripts/nfr-open-bench.sh <binary_path> [iterations]
# Defaults to 30 iterations (the spec NFR-1 sample size; 10 was a
# single-outlier p95).
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
for tool in xdotool nc ss; do
  if ! command -v "$tool" &>/dev/null; then
    echo "nfr-open-bench: '$tool' not found — install it" >&2
    exit 1
  fi
done

# Start the app once; keep it alive for all iterations (prewarm scenario).
"$BINARY" &
APP_PID=$!
trap 'kill "$APP_PID" 2>/dev/null || true' EXIT

# Wait for the toggle listener to bind (up to READY_TIMEOUT seconds). The app
# binds the UDP port early in setup(), but on a headless software-rendered
# (llvmpipe) CI runner the surrounding GTK/webview init is slow, so allow
# generous headroom. Probe the kernel socket table with `ss` (authoritative
# for a bound UNCONN UDP socket) rather than `nc -zu`, whose UDP "connect"
# result is unreliable.
READY_TIMEOUT="${READY_TIMEOUT:-45}"
echo "Waiting for app to bind toggle port :${TOGGLE_PORT} (up to ${READY_TIMEOUT}s)…"
deadline=$((SECONDS + READY_TIMEOUT))
until ss -lunH "sport = :${TOGGLE_PORT}" 2>/dev/null | grep -q .; do
  if ! kill -0 "$APP_PID" 2>/dev/null; then
    echo "nfr-open-bench: app process exited before binding toggle port" >&2
    exit 1
  fi
  if [ $SECONDS -ge $deadline ]; then
    echo "nfr-open-bench: app did not bind toggle port within ${READY_TIMEOUT}s" >&2
    echo "  app alive: $(kill -0 "$APP_PID" 2>/dev/null && echo yes || echo no)" >&2
    echo "  udp :${TOGGLE_PORT}: $(ss -lunH "sport = :${TOGGLE_PORT}" 2>/dev/null || echo none)" >&2
    echo "  process tree: $(pgrep -a hotdoc 2>/dev/null | tr '\n' '|')" >&2
    exit 1
  fi
  sleep 0.2
done

# The window is created `visible: false` and is not realized in the X tree
# until the first `.show()`, so it is initially unfindable by xdotool. Send
# one toggle to realize + show it, then wait for it to appear.
printf '\x01' | nc -u -w1 127.0.0.1 "$TOGGLE_PORT" &>/dev/null || true
deadline=$((SECONDS + 20))
until xdotool search --name "hotdoc" &>/dev/null; do
  if [ $SECONDS -ge $deadline ]; then
    echo "nfr-open-bench: window not found within 20s of first toggle" >&2
    echo "  app alive: $(kill -0 "$APP_PID" 2>/dev/null && echo yes || echo no)" >&2
    echo "  windows: $(xdotool search --name '' 2>/dev/null | tr '\n' ' ')" >&2
    exit 1
  fi
  # Re-send the toggle each second in case the first datagram raced the
  # listener's first recv.
  printf '\x01' | nc -u -w1 127.0.0.1 "$TOGGLE_PORT" &>/dev/null || true
  sleep 0.5
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
