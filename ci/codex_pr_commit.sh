#!/usr/bin/env bash
set -euo pipefail

if [ "${HEAD_REPO}" != "${BASE_REPO}" ]; then
  echo "Fork PR detected, skipping push."
  exit 0
fi

if git diff --quiet && [ ! -f CODEX_CI_FIX_REPORT.md ]; then
  echo "No Codex changes to commit."
  exit 0
fi

git config user.name "github-actions[bot]"
git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
git add -A
git commit -m "chore(ci): codex pre-fix for lint/build/test" || echo "No changes to commit"
git push origin "HEAD:${BRANCH}"
