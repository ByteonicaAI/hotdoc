#!/usr/bin/env bash
# NFR-7 (T8, T11): zero outbound non-loopback connections during normal use.
#
# Attributes network activity to the APP PROCESS TREE via `ss -p`, polling
# the app's (and its WebKit children's) sockets throughout the soak. If any
# socket owned by the tree ever has a non-loopback local or peer address, the
# gate fails.
#
# Why process-attributed, not host-wide tcpdump: the previous implementation
# ran `tcpdump -i any` with no process filter, capturing ALL host traffic.
# On a GitHub Actions runner the host is constantly talking to GitHub
# (140.82.0.0/16:443) for job coordination and log upload, so the gate always
# saw hundreds of non-loopback packets that had nothing to do with hotdoc.
# Scoping to the app's own sockets removes those false positives while still
# catching a genuine phone-home (autoupdate, telemetry, crash ping), which
# would appear as a non-loopback socket owned by the app.
#
# The only sockets the app legitimately holds are loopback: the UDP toggle on
# 127.0.0.1:47474, plus X11/D-Bus which use Unix sockets (not IP), so they
# never appear here.
#
# Soak length — accepted deviation per M4.5-T11 Option A:
#   - CI uses 60s (the explicit second arg from ci.yml overrides the default).
#     60s catches the startup-period traffic where most leaks live.
#   - Local direct invocation defaults to 600s (10 min) — matches spec
#     §13/NFR-7. Pre-release manual smoke MUST use the 600s default.
#
# Usage: scripts/nfr-network.sh <binary_path> [soak_seconds]
# Requires: ss (iproute2), Xvfb (DISPLAY set), nc. No root needed — `ss -p`
# sees the invoking user's own processes without privilege.
set -euo pipefail

BINARY="${1:-target/release/hotdoc}"
SOAK="${2:-600}"  # spec §13/NFR-7: 10 min pre-release smoke; CI overrides with 60
TOGGLE_PORT=47474
POLL_INTERVAL="${POLL_INTERVAL:-0.5}"

if [ ! -x "$BINARY" ]; then
  echo "nfr-network: binary '$BINARY' not found" >&2
  exit 1
fi
for tool in ss nc; do
  if ! command -v "$tool" &>/dev/null; then
    echo "nfr-network: '$tool' not found — install it" >&2
    exit 1
  fi
done

VIOLATIONS=$(mktemp /tmp/hotdoc-nfr7-XXXXXX)
"$BINARY" &
APP_PID=$!
trap 'rm -f "$VIOLATIONS"; kill "$APP_PID" 2>/dev/null || true' EXIT

# All pids in the app tree (app + WebKit web/network processes, tray helper…).
collect_pids() {
  local pids=("$APP_PID") queue=("$APP_PID") next p c
  while [ "${#queue[@]}" -gt 0 ]; do
    next=()
    for p in "${queue[@]}"; do
      while IFS= read -r c; do
        [ -n "$c" ] || continue
        case " ${pids[*]} " in *" $c "*) ;; *) pids+=("$c"); next+=("$c") ;; esac
      done < <(pgrep -P "$p" 2>/dev/null || true)
    done
    queue=("${next[@]}")
  done
  printf '%s\n' "${pids[@]}"
}

# True if an "address:port" token is loopback, a wildcard, or a listener bind
# (i.e. not an outbound non-loopback endpoint).
is_local_or_wildcard() {
  case "$1" in
    127.0.0.*|\[::1\]:*|\[::ffff:127.0.0.1\]:*) return 0 ;;  # loopback
    *\**|0.0.0.0:*|\[::\]:*|"") return 0 ;;                  # wildcard / listener
    *) return 1 ;;
  esac
}

echo "NFR-7: monitoring app sockets for ${SOAK}s (pid=$APP_PID)…"
soak_end=$(( SECONDS + SOAK ))
iteration=0
while [ "$SECONDS" -lt "$soak_end" ]; do
  if ! kill -0 "$APP_PID" 2>/dev/null; then
    echo "nfr-network: app exited during soak" >&2
    break
  fi

  # Build a "pid=N," alternation for the current tree (trailing comma avoids
  # pid=12 matching pid=123). ss prints Process as ...,pid=N,fd=M)).
  pid_re=$(collect_pids | sed 's/^/pid=/;s/$/,/' | paste -sd'|' -)
  if [ -n "$pid_re" ]; then
    # -t tcp, -u udp, -a all states, -n numeric, -p process, -H no header.
    while IFS= read -r line; do
      [ -n "$line" ] || continue
      local_addr=$(awk '{print $5}' <<<"$line")
      peer_addr=$(awk '{print $6}' <<<"$line")
      if ! is_local_or_wildcard "$local_addr" || ! is_local_or_wildcard "$peer_addr"; then
        echo "$line" >> "$VIOLATIONS"
      fi
    done < <(ss -tuanpH state all 2>/dev/null | grep -E "$pid_re" || true)
  fi

  if (( iteration % 10 == 0 )); then
    printf '\x01' | nc -u -w1 127.0.0.1 "$TOGGLE_PORT" &>/dev/null || true
  fi
  sleep "$POLL_INTERVAL"
  iteration=$(( iteration + 1 ))
done

kill "$APP_PID" 2>/dev/null || true
wait "$APP_PID" 2>/dev/null || true

COUNT=$(sort -u "$VIOLATIONS" 2>/dev/null | grep -c '^' || true)
COUNT=${COUNT:-0}
echo "NFR-7: ${COUNT} distinct non-loopback app socket(s) observed during ${SOAK}s drive"
if [ "$COUNT" -gt 0 ]; then
  echo "NFR-7 FAIL: app opened non-loopback socket(s):"
  sort -u "$VIOLATIONS" | head -20
  exit 1
fi
echo "NFR-7 PASS"
