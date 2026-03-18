#!/usr/bin/env bash
set -euo pipefail

if git diff --name-only | grep -E '^(tests/|src/tests/)|(^|/).*\.snap$'; then
  echo "Codex changed test files/snapshots, which is forbidden."
  exit 1
fi
