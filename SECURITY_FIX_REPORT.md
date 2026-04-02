# SECURITY_FIX_REPORT

## Summary
- Reviewed all provided alerts (`dependabot: 0`, `code_scanning: 9` in scope).
- Applied minimal workflow hardening and supply-chain fixes in the referenced workflow files.
- Replaced or pinned third-party actions to immutable commit SHAs where applicable.

## Remediations Applied

### 1) `actions/unpinned-tag` in `.github/workflows/dev-publish.yml`
- Pinned `aws-actions/configure-aws-credentials@v4` to:
  - `aws-actions/configure-aws-credentials@d0834ad3a60a024346910e522a81b0002bd37fea`
- Pinned `gittools/actions/gitversion/setup@v1` to:
  - `gittools/actions/gitversion/setup@dcb17efb49ec7f20efdebce79cc397a3952c63db`
- Pinned `gittools/actions/gitversion/execute@v1` to:
  - `gittools/actions/gitversion/execute@dcb17efb49ec7f20efdebce79cc397a3952c63db`

### 2) `actions/unpinned-tag` in `.github/workflows/publish.yml`
- Replaced third-party `katyo/publish-crates@v2` with native command execution:
  - `cargo publish --allow-dirty --token "$CARGO_REGISTRY_TOKEN"`
- Pinned `softprops/action-gh-release@v2` to:
  - `softprops/action-gh-release@153bb8e04406b158c6c84fc1615b65b24149a1fe`

### 3) `actions/unpinned-tag` in `.github/workflows/nightly-wizard-e2e.yml`
- Replaced third-party `taiki-e/install-action@cargo-binstall` with native command execution:
  - `cargo install cargo-binstall --locked`

## Alert Reconciliation Notes

### `actions/code-injection/medium` in `.github/workflows/codex-security-fix.yml`
- The currently checked-in workflow no longer contains the flagged inline interpolation pattern (`${{ github.event.pull_request.head.ref }}` in a `run:` context).
- Current file is a thin caller to a reusable workflow with a fork-PR guard already present.

### `actions/unpinned-tag` for `openai/codex-action@v1` in `.github/workflows/codex-security-fix.yml`
- The currently checked-in file does not directly invoke `openai/codex-action`.
- This alert appears to target historical workflow content rather than the current file body in this branch.

## Files Updated
- `.github/workflows/dev-publish.yml`
- `.github/workflows/publish.yml`
- `.github/workflows/nightly-wizard-e2e.yml`
- `SECURITY_FIX_REPORT.md`
