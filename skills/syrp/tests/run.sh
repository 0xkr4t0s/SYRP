#!/usr/bin/env bash
# Run the skill's own tests: one script per spec behaviour (b1_ ... b4_). Exits 1 if any fails.
# Run from anywhere inside the repository: skills/syrp/tests/run.sh
set -uo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
failed=0
for t in "$here"/b[0-9]*_*.sh; do
  name="$(basename "$t" .sh)"
  if bash "$t"; then echo "ok   $name"; else echo "FAIL $name"; failed=1; fi
done
exit "$failed"
