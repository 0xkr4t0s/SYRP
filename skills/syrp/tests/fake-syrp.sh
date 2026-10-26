#!/usr/bin/env bash
# Stand-in for `cargo syrp` used by the B2 test. FAKE_<CMD>_EXIT sets each command's exit code;
# FAKE_<CMD>_RAW=1 prints text instead of an envelope.
cmd="$1"; shift
up="$(tr '[:lower:]' '[:upper:]' <<<"$cmd")"
code_var="FAKE_${up}_EXIT"; raw_var="FAKE_${up}_RAW"
code="${!code_var:-0}"
if [ "${!raw_var:-0}" = 1 ]; then echo "error: could not compile"; exit "$code"; fi
jq -cn --arg cmd "$cmd" --argjson code "$code" --arg args "$*" \
  '{schema: "syrp.envelope/v1", command: $cmd, ok: ($code == 0), exit_code: $code,
    results: [{args: $args}],
    diagnostics: (if $code == 0 then [] else [{code: "SYRP-CLI-0301", severity: "error", message: "boom"}] end)}'
exit "$code"
