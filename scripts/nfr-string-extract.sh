#!/usr/bin/env bash
# NFR-10: hard-coded aria-label or placeholder strings in .svelte files.
# Static string = no {expression} interpolation; dynamic = allowed.
# Fail if any .svelte file in src/ contains a static aria-label or
# placeholder attribute value.
#
# The regex below matches `"<value>"` where <value> contains no `{`
# (open brace). Any quoted value that contains a `{` is considered a
# dynamic interpolation and is allowed.
#
# ponytail: extracted from ci.yml:53-66 so the local pnpm run
# string:check and CI share one source of truth.
set -euo pipefail

VIOLATIONS=$(grep -Prn 'aria-label="[^{\"]*"' src/ --include='*.svelte' || true)
if [ -n "$VIOLATIONS" ]; then
  echo "NFR-10 FAIL: hard-coded aria-label strings:" >&2
  echo "$VIOLATIONS" >&2
  exit 1
fi

VIOLATIONS=$(grep -Prn 'placeholder="[^{\"]*"' src/ --include='*.svelte' || true)
if [ -n "$VIOLATIONS" ]; then
  echo "NFR-10 FAIL: hard-coded placeholder strings:" >&2
  echo "$VIOLATIONS" >&2
  exit 1
fi

echo "NFR-10 PASS"