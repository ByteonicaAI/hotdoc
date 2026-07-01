#!/usr/bin/env bash
# NFR-4 (T5): idle PSS < 350 MB after a soak.
# A short CI soak catches runaway allocators and memory leaks in the
# startup path; sustained 5-minute idle is a local dev gate.
#
# Measures PSS (Proportional Set Size), NOT summed RSS. The WebKitGTK
# webview runs as a 3-process tree (UI + WebProcess + NetworkProcess)
# that shares ~160 MB of libraries (libwebkit2gtk, libjavascriptcore,
# libicu, libgtk). Summing VmRSS counts those shared pages once per
# process (~3x), inflating the figure to ~465 MB of phantom memory. PSS
# divides each shared page's cost across the processes mapping it, so the
# tree total reflects the real physical footprint.
#
# Budget recalibrated 2026-07-01 from 250 -> 350 MB (owner decision). The
# original 250 MB target predated a real PSS measurement of the app. The
# app prewarms the webview at launch for fast opens (NFR-1), so idle PSS is
# WebKitGTK's prewarmed-engine floor: ~206 MB on a GPU host, ~298 MB on the
# headless software-rendered (llvmpipe) CI runner. WebKit-tuning env knobs
# (disable compositing/DMABUF/JIT) move it <5%. 350 MB = ~17% headroom over
# the CI floor for runner variance while still catching a genuine leak,
# which grows unbounded over the soak. See
# docs/internal/2026-07-01-packaging-deb-only.md.
#
# Usage: scripts/nfr-memory.sh <binary_path>
#   binary_path — path to the hotdoc release binary (workspace-root target)
# Requires: Xvfb already running on $DISPLAY (or DISPLAY set by caller).
set -euo pipefail

BINARY="${1:-target/release/hotdoc}"
LIMIT_KB=$(( 350 * 1024 ))  # 350 MB in kB
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
  echo "nfr-memory: app exited immediately — cannot measure PSS" >&2
  exit 1
fi

echo "NFR-4: soaking for ${SOAK_SECONDS}s (pid=$APP_PID)…"
sleep "$SOAK_SECONDS"

if ! kill -0 "$APP_PID" 2>/dev/null; then
  echo "nfr-memory: app exited during soak" >&2
  exit 1
fi

PSS_KB=0
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
  # smaps_rollup's Pss is the per-process proportional share of physical
  # memory (private pages + shared pages / number of sharers). Summing it
  # across the tree gives the true footprint without triple-counting the
  # shared WebKit libraries.
  pss=$(awk '/^Pss:/{s+=$2} END{print s+0}' "/proc/$p/smaps_rollup" 2>/dev/null || echo 0)
  PSS_KB=$(( PSS_KB + ${pss:-0} ))
done
kill "$APP_PID" 2>/dev/null || true
wait "$APP_PID" 2>/dev/null || true

PSS_MB=$(( PSS_KB / 1024 ))
echo "NFR-4 idle PSS (tree total, ${#PIDS[@]} procs): ${PSS_KB}kB (${PSS_MB}MB), limit ${LIMIT_KB}kB (350MB)"

if [ "$PSS_KB" -gt "$LIMIT_KB" ]; then
  echo "NFR-4 FAIL: idle PSS ${PSS_MB}MB > 350MB"
  exit 1
fi
echo "NFR-4 PASS"
