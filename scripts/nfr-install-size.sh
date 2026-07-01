#!/usr/bin/env bash
# NFR-5 (T6): install size — .deb ≤ 30 MB.
# v1.0 ships the .deb only (it depends on system libwebkit2gtk-4.1, ~4 MB).
# A self-contained AppImage bundles all of WebKitGTK (~80 MB) and cannot
# meet a 30 MB budget; it is deferred to v1.1 with a webkit-realistic
# budget. See docs/internal/2026-07-01-packaging-deb-only.md.
# Usage: scripts/nfr-install-size.sh <bundle_dir>
#   bundle_dir — path to the tauri bundle directory (workspace-root target)
set -euo pipefail

BUNDLE_DIR="${1:-target/release/bundle}"
LIMIT_BYTES=$(( 30 * 1024 * 1024 ))  # 30 MB

found_any=0
fail=0

dir="$BUNDLE_DIR/deb"
if [ -d "$dir" ]; then
  while IFS= read -r -d '' artifact; do
    found_any=1
    size=$(wc -c < "$artifact")
    size_mb=$(( size / 1024 / 1024 ))
    if [ "$size" -gt "$LIMIT_BYTES" ]; then
      echo "NFR-5 FAIL: $artifact is ${size_mb}MB > 30MB"
      fail=1
    else
      echo "NFR-5 OK:   $artifact is ${size_mb}MB"
    fi
  done < <(find "$dir" -name "*.deb" -print0 2>/dev/null)
fi

# A missing .deb is a real failure, not a skip — the gate must not pass
# vacuously when the expected artifact was never produced.
if [ "$found_any" -eq 0 ]; then
  echo "NFR-5 FAIL: no .deb found under '$BUNDLE_DIR/deb'" >&2
  exit 1
fi

[ "$fail" -eq 0 ] && echo "NFR-5 PASS"
exit "$fail"
