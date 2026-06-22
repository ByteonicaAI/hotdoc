#!/usr/bin/env bash
# NFR-6 (T7): tantivy index size ≤ ~3 MB per pack.
# Usage: scripts/nfr-index-size.sh <index_dir> <packs_dir>
#   index_dir  — the tantivy index produced by `hotdoc-cli index --out`
#   packs_dir  — directory containing *.json pack files (for pack count)
set -euo pipefail

INDEX_DIR="${1:-src-tauri/target/bench-index}"
PACKS_DIR="${2:-packs/curate}"
LIMIT_PER_PACK_BYTES=$(( 3 * 1024 * 1024 ))  # 3 MB per pack

if [ ! -d "$INDEX_DIR" ]; then
  echo "nfr-index-size: index dir '$INDEX_DIR' not found" >&2
  exit 1
fi

INDEX_BYTES=$(du -sb "$INDEX_DIR" | cut -f1)
PACK_COUNT=$(find "$PACKS_DIR" -maxdepth 1 -name "*.json" 2>/dev/null | wc -l)

if [ "$PACK_COUNT" -eq 0 ]; then
  echo "nfr-index-size: no packs found in '$PACKS_DIR'" >&2
  exit 1
fi

LIMIT_BYTES=$(( PACK_COUNT * LIMIT_PER_PACK_BYTES ))
INDEX_MB=$(( INDEX_BYTES / 1024 / 1024 ))
LIMIT_MB=$(( LIMIT_BYTES / 1024 / 1024 ))

echo "NFR-6 index-size: ${INDEX_BYTES}B (${INDEX_MB}MB) for ${PACK_COUNT} packs, limit ${LIMIT_BYTES}B (${LIMIT_MB}MB)"

if [ "$INDEX_BYTES" -gt "$LIMIT_BYTES" ]; then
  echo "NFR-6 FAIL: index ${INDEX_MB}MB exceeds ${LIMIT_MB}MB (${PACK_COUNT} packs × 3 MB/pack)"
  exit 1
fi
echo "NFR-6 PASS"
