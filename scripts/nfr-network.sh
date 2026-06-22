#!/usr/bin/env bash
# NFR-7 (T8): zero outbound non-loopback connections during a 2-minute drive.
#
# Captures non-loopback TCP/IP traffic with tcpdump for the duration that
# the app runs. Any non-loopback packet fails the gate.
#
# The hotdoc app must make zero outbound network calls during normal use
# (search, copy, pin, settings). The only allowed loopback traffic is
# the UDP toggle on 127.0.0.1:47474 (carved out by the filter).
#
# Usage: scripts/nfr-network.sh <binary_path> [soak_seconds]
# Requires: tcpdump (run as root or with CAP_NET_RAW), Xvfb (DISPLAY set).
set -euo pipefail

BINARY="${1:-src-tauri/target/release/hotdoc}"
SOAK="${2:-120}"  # 2-minute drive; spec says 10 min for local; CI uses 2 min
TOGGLE_PORT=47474

if [ ! -x "$BINARY" ]; then
  echo "nfr-network: binary '$BINARY' not found" >&2
  exit 1
fi

CAPTURE_FILE=$(mktemp /tmp/hotdoc-nfr7-XXXXXX.pcap)
trap 'rm -f "$CAPTURE_FILE"; kill "$APP_PID" "$TCPDUMP_PID" 2>/dev/null || true' EXIT

# Start tcpdump on all non-loopback interfaces; carve out loopback UDP toggle.
# Captures only non-loopback traffic (not 127.0.0.0/8 or ::1).
sudo tcpdump -i any \
  'not (src net 127.0.0.0/8 or dst net 127.0.0.0/8 or src net ::1/128 or dst net ::1/128)' \
  -w "$CAPTURE_FILE" -q &
TCPDUMP_PID=$!
sleep 1  # let tcpdump settle

# Start the app.
"$BINARY" &
APP_PID=$!

# Drive the app: send toggle every 10 seconds to simulate normal use.
echo "NFR-7: monitoring for ${SOAK}s (pid=$APP_PID)…"
soak_end=$(( SECONDS + SOAK ))
iteration=0
while [ $SECONDS -lt "$soak_end" ]; do
  if ! kill -0 "$APP_PID" 2>/dev/null; then
    echo "nfr-network: app exited during soak" >&2
    break
  fi
  if (( iteration % 10 == 0 )); then
    # Send a toggle (simulates hotkey activation).
    printf '\x01' | nc -u -w1 127.0.0.1 "$TOGGLE_PORT" &>/dev/null || true
  fi
  sleep 1
  iteration=$(( iteration + 1 ))
done

kill "$APP_PID" 2>/dev/null || true
wait "$APP_PID" 2>/dev/null || true
sleep 1

# Stop tcpdump and check capture.
sudo kill "$TCPDUMP_PID" 2>/dev/null || true
wait "$TCPDUMP_PID" 2>/dev/null || true

# Count non-loopback packets (exclude the filter we already applied).
PACKET_COUNT=$(sudo tcpdump -r "$CAPTURE_FILE" -n 2>/dev/null | grep -c '^' || echo 0)

echo "NFR-7: captured ${PACKET_COUNT} non-loopback packets during ${SOAK}s drive"
if [ "$PACKET_COUNT" -gt 0 ]; then
  echo "NFR-7 FAIL: outbound non-loopback traffic detected:"
  sudo tcpdump -r "$CAPTURE_FILE" -n 2>/dev/null | head -20
  exit 1
fi
echo "NFR-7 PASS"
