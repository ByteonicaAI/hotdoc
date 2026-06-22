#!/usr/bin/env bash
# NFR-5 (T6): install size — AppImage + .deb each ≤ 30 MB.
# Usage: scripts/nfr-install-size.sh <bundle_dir>
#   bundle_dir — path to the tauri bundle directory
#                (e.g. src-tauri/target/release/bundle)
set -euo pipefail

BUNDLE_DIR="${1:-src-tauri/target/release/bundle}"
LIMIT_BYTES=$(( 30 * 1024 * 1024 ))  # 30 MB

found_any=0
fail=0

for bundle_type in appimage deb; do
  dir="$BUNDLE_DIR/$bundle_type"
  if [ ! -d "$dir" ]; then
    continue
  fi
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
  done < <(find "$dir" \( -name "*.AppImage" -o -name "*.deb" \) -print0 2>/dev/null)
done

if [ "$found_any" -eq 0 ]; then
  echo "NFR-5: no AppImage or .deb found under '$BUNDLE_DIR' — skipping" >&2
  exit 0
fi

[ "$fail" -eq 0 ] && echo "NFR-5 PASS"
exit "$fail"
