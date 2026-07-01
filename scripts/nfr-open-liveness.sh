#!/usr/bin/env bash
# NFR-1 (T4) headless liveness gate — the open PATH works end to end without
# measuring paint latency.
#
# Why this exists (and why it is not the full open-time bench): NFR-1's real
# metric is warm-open latency (window visible ≤150ms p50 after a toggle).
# Measuring it needs a mapped, painted window, which requires a GPU/compositor.
# A headless CI runner (Xvfb, software llvmpipe, no compositor) cannot map the
# transparent `main` window at all — WebKitGTK never realizes a top-level X
# window, so xdotool can neither find nor time it. The latency bench
# (scripts/nfr-open-bench.sh) therefore runs on a real display (developer
# machine / release QA via `verify:all`), NOT in headless CI.
#
# What CI CAN verify headlessly is that the open PATH is wired correctly: the
# app binds the toggle listener and, on a toggle datagram, runs the show path
# and acks. A regression that breaks the toggle wiring (wrong port, listener
# not spawned, panic in the handler) fails here. Latency regressions are caught
# by the real-display bench.
#
# Usage: scripts/nfr-open-liveness.sh <binary_path>
# Requires: Xvfb (DISPLAY set), ss (iproute2), netcat (nc). No window manager.
set -euo pipefail

BINARY="${1:-target/release/hotdoc}"
TOGGLE_PORT=47474
READY_TIMEOUT="${READY_TIMEOUT:-45}"

if [ ! -x "$BINARY" ]; then
  echo "nfr-open-liveness: binary '$BINARY' not found" >&2
  exit 1
fi
for tool in nc ss; do
  if ! command -v "$tool" &>/dev/null; then
    echo "nfr-open-liveness: '$tool' not found — install it" >&2
    exit 1
  fi
done

"$BINARY" &
APP_PID=$!
trap 'kill "$APP_PID" 2>/dev/null || true' EXIT

echo "Waiting for app to bind toggle port :${TOGGLE_PORT} (up to ${READY_TIMEOUT}s)…"
deadline=$((SECONDS + READY_TIMEOUT))
until ss -lunH "sport = :${TOGGLE_PORT}" 2>/dev/null | grep -q .; do
  if ! kill -0 "$APP_PID" 2>/dev/null; then
    echo "NFR-1 liveness FAIL: app process exited before binding toggle port" >&2
    exit 1
  fi
  if [ $SECONDS -ge $deadline ]; then
    echo "NFR-1 liveness FAIL: app did not bind toggle port within ${READY_TIMEOUT}s" >&2
    echo "  udp :${TOGGLE_PORT}: $(ss -lunH "sport = :${TOGGLE_PORT}" 2>/dev/null || echo none)" >&2
    exit 1
  fi
  sleep 0.2
done
echo "toggle port bound"

# Send a toggle and require the ack back. The listener acks AFTER running the
# show path, so a returned datagram proves the handler executed end to end.
ack=$(printf '\x01' | nc -u -w2 127.0.0.1 "$TOGGLE_PORT" 2>/dev/null | head -c1 | xxd -p 2>/dev/null || true)
if [ "$ack" = "01" ]; then
  echo "NFR-1 liveness PASS: toggle acked (show path ran)"
  exit 0
fi

echo "NFR-1 liveness FAIL: no ack from toggle listener (got '${ack:-<none>}')" >&2
exit 1
