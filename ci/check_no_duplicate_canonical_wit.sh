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

PATTERN='^package\s+greentic:component@'
PATTERN_ERE='^package[[:space:]]+greentic:component@'

# rg exit codes: 0 = matches found (violation), 1 = no matches (OK), other = error.
rc=0
if [[ "$SEARCH_TOOL" == "rg" ]]; then
  MATCHES="$(rg -n --hidden --glob '!.git/*' --glob '*.wit' \
    --glob '!**/target/**' \
    "$PATTERN" . )" || rc=$?
else
  MATCHES="$(grep -R -n -E --include='*.wit' --exclude-dir=.git --exclude-dir=target \
    "$PATTERN_ERE" . )" || rc=$?
fi
if [[ "$rc" -gt 1 ]]; then
  echo "ERROR: rg failed with exit code $rc" >&2
  exit "$rc"
fi

if [[ "$rc" -eq 0 && -n "$MATCHES" ]]; then
  echo "ERROR: greentic-flow must not define canonical greentic:component WIT."
  echo
  echo "$MATCHES"
  exit 1
fi

echo "OK: No canonical greentic:component WIT found."
