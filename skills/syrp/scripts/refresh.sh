#!/usr/bin/env bash
# Regenerate references/errors.json from the CLI's error catalogue.
# Uses `cargo syrp docs build --skip-book` (agent side only) into a temporary directory and
# rewrites the array one entry per line, so diffs stay small. Run it after adding error codes.
#
#   scripts/refresh.sh              # rewrite references/errors.json
#   scripts/refresh.sh <file>       # write elsewhere (the freshness test uses this)
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out="$(mktemp -d)"
trap 'rm -rf "$out"' EXIT
dest="${1:-$here/references/errors.json}"
(cd "$here" && cargo syrp docs build --skip-book --out "$out" --json >/dev/null)
{
  echo '['
  jq -c '.[]' "$out/agents/errors.json" | sed '$!s/$/,/'
  echo ']'
} >"$dest"
jq -e 'length > 0' "$dest" >/dev/null
echo "wrote $dest ($(jq length "$dest") codes)"
