# Security Fix Report

Date: 2026-04-02 (UTC)

## Scope
Reviewed provided security alerts:
- Code scanning alert #12 (`actions/unpinned-tag`) for `softprops/action-gh-release@v2` in `.github/workflows/publish.yml`
- Code scanning alert #8 (`actions/unpinned-tag`) for `katyo/publish-crates@v2` in `.github/workflows/publish.yml`

## Findings and Remediation
1. Alert #12 (`softprops/action-gh-release`): **Remediated in current workflow**.
- Current file uses a full commit SHA pin:
  - `.github/workflows/publish.yml`: `softprops/action-gh-release@153bb8e04406b158c6c84fc1615b65b24149a1fe # v2`
- This satisfies the CodeQL recommendation to pin third-party actions to immutable commit SHAs.

2. Alert #8 (`katyo/publish-crates`): **No vulnerable reference present in current workflow**.
- `katyo/publish-crates` is not present in `.github/workflows/publish.yml`.
- The currently committed workflow no longer contains the unpinned tag usage referenced by the alert metadata.

## Files Changed
- Added `SECURITY_FIX_REPORT.md` (this report).

## Verification Performed
- Searched workflows for relevant action references and unpinned `@vN` patterns.
- Confirmed `.github/workflows/publish.yml` contains a pinned SHA for `softprops/action-gh-release` and no `katyo/publish-crates` usage.

## Residual Risk
- Other workflows/composite actions may still contain unpinned third-party actions not covered by the two specified alerts (e.g., `Swatinem/rust-cache@v2`, `taiki-e/install-action@v2`). These were not part of the provided alert scope.
