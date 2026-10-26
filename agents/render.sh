#!/usr/bin/env bash
# Fills {{KEY}} placeholders in a role prompt. Usage: render.sh <template> KEY=value ...
set -euo pipefail
template="$(dirname "$0")/$1"; shift
text="$(cat "$template")"
for kv in "$@"; do
  key="${kv%%=*}"; value="${kv#*=}"
  text="${text//\{\{$key\}\}/$value}"
done
printf '%s\n' "$text"
