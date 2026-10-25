#!/usr/bin/env bash
set -euo pipefail

if command -v rg >/dev/null 2>&1; then
  SEARCH_TOOL=rg
elif command -v grep >/dev/null 2>&1; then
  # Fallback with equivalent patterns (grep -E); never weaker than rg since it ignores .gitignore.
  SEARCH_TOOL=grep
else
  echo "ERROR: ripgrep (rg) or grep is required but neither is installed" >&2
  exit 2
fi

# rg exit codes: 0 = matches found (violation), 1 = no matches (OK), other = error.
rc=0
if [[ "$SEARCH_TOOL" == "rg" ]]; then
  rg -n "greentic_interfaces::bindings::|\bbindings::greentic::" src tests docs README.md || rc=$?
else
  grep -R -n -E "greentic_interfaces::bindings::|\bbindings::greentic::" src tests docs README.md || rc=$?
fi || rc=$?
case "$rc" in
  0)
    echo "ERROR: use greentic_interfaces::canonical instead of bindings::* in downstream code/docs"
    exit 1
    ;;
  1) ;;
  *)
    echo "ERROR: rg failed with exit code $rc" >&2
    exit "$rc"
    ;;
esac

echo "OK: no greentic_interfaces::bindings usage in src/tests/docs/README."
