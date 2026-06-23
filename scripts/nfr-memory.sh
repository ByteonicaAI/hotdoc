#!/usr/bin/env bash
# NFR-4 (T5): idle RSS < 250 MB after a 30-second soak.
# A 30-second CI soak catches runaway allocators and memory leaks
# in the startup path; sustained 5-minute idle is a local dev gate.
# Usage: scripts/nfr-memory.sh <binary_path>
#   binary_path — path to the hotdoc release binary
# Requires: Xvfb already running on $DISPLAY (or DISPLAY set by caller).
set -euo pipefail

BINARY="${1:-src-tauri/target/release/hotdoc}"
LIMIT_KB=$(( 250 * 1024 ))  # 250 MB in kB
SOAK_SECONDS="${SOAK_SECONDS:-30}"

if [ ! -x "$BINARY" ]; then
  echo "nfr-memory: binary '$BINARY' not found or not executable" >&2
  exit 1
fi

# Start app; tray apps may exit 0 if another instance is already running.
"$BINARY" &
APP_PID=$!

# Wait for the app to settle (window created, tray registered).
sleep 5

if ! kill -0 "$APP_PID" 2>/dev/null; then
  echo "nfr-memory: app exited immediately — cannot measure RSS" >&2
  exit 1
fi

echo "NFR-4: soaking for ${SOAK_SECONDS}s (pid=$APP_PID)…"
sleep "$SOAK_SECONDS"

if ! kill -0 "$APP_PID" 2>/dev/null; then
  echo "nfr-memory: app exited during soak" >&2
  exit 1
fi

RSS_KB=0
PIDS=("$APP_PID")
queue=("$APP_PID")
while [ "${#queue[@]}" -gt 0 ]; do
  next=()
  for p in "${queue[@]}"; do
    while IFS= read -r c; do
      [ -n "$c" ] || continue
      case " ${PIDS[*]} " in *" $c "*) ;; *) PIDS+=("$c"); next+=("$c") ;; esac
    done < <(pgrep -P "$p" 2>/dev/null || true)
  done
  queue=("${next[@]}")
done
for p in "${PIDS[@]}"; do
  rss=$(awk '/^VmRSS:/{print $2}' "/proc/$p/status" 2>/dev/null || echo 0)
  RSS_KB=$(( RSS_KB + ${rss:-0} ))
done
kill "$APP_PID" 2>/dev/null || true
wait "$APP_PID" 2>/dev/null || true

RSS_MB=$(( RSS_KB / 1024 ))
echo "NFR-4 idle RSS (tree total, ${#PIDS[@]} procs): ${RSS_KB}kB (${RSS_MB}MB), limit ${LIMIT_KB}kB (250MB)"

if [ "$RSS_KB" -gt "$LIMIT_KB" ]; then
  echo "NFR-4 FAIL: idle RSS ${RSS_MB}MB > 250MB"
  exit 1
fi
echo "NFR-4 PASS"
