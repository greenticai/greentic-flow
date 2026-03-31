# Security Fix Report

Date: 2026-03-31 (UTC)

## Scope Reviewed
- Alerts input: 0 Dependabot, 12 CodeQL alerts.
- PR dependency vulnerability input: `[]` (none reported).
- Repository check for dependency-file regressions: no dependency vulnerability entries were provided, and no dependency manifest changes were introduced by this remediation.

## Fixes Applied

### 1) High severity path-injection hardening (`build.rs`)
- Alert reference: CodeQL #14 (`rust/path-injection`), `build.rs:23`.
- Change made:
  - Replaced runtime `CARGO_MANIFEST_DIR` environment read with compile-time `env!("CARGO_MANIFEST_DIR")`.
  - Added explicit rejection of `..` segments in `CARGO_TARGET_DIR` before path construction.
- Security effect:
  - Reduces trust on mutable runtime environment for manifest root resolution.
  - Strengthens traversal prevention for target directory input.

## Alerts Evaluated As Already Remediated In Current Tree

### Workflow alerts
- #73 (`actions/code-injection/medium`) in `.github/workflows/codex-security-fix.yml`
  - Current workflow no longer interpolates `${{ github.event.pull_request.head.ref }}` in shell context.
  - Uses validated `PR_HEAD_SHA` with strict 40-hex check before API use.
- #72 (`actions/unpinned-tag`) in `.github/workflows/codex-security-fix.yml`
  - `openai/codex-action` is pinned to full commit SHA.
- #11 (`actions/unpinned-tag`) in `.github/workflows/dev-publish.yml`
  - Reported `gittools/actions/gitversion/execute@v1` usage is not present.
- #7 (`actions/unpinned-tag`) in `.github/workflows/nightly-wizard-e2e.yml`
  - Reported `taiki-e/install-action@cargo-binstall` usage is not present.
- #12 (`actions/unpinned-tag`) in `.github/workflows/publish.yml`
  - Reported `softprops/action-gh-release@v2` usage is not present.

### Rust alerts
- #68 (`rust/request-forgery`) in `src/bin/greentic-flow.rs`
  - Current logic uses fixed release URL + HTTPS scheme enforcement + host allowlist + redirect host/scheme constraints.
- #71, #69 (`rust/log-injection`) in `src/bin/greentic-flow.rs` and `tests/add_step_integration.rs`
  - Test location already sanitizes untrusted value before logging.
  - Reported binary line mapping appears stale relative to current source.
- #67, #16, #17, #23 (`rust/path-injection`) in `src/bin/greentic-flow.rs`, `src/cache/disk.rs`, `src/component_schema.rs`, `src/loader.rs`
  - Current code contains traversal/normalization/canonicalization guards at the reported paths; line mappings appear to reference older code states.

## Validation
- Ran: `cargo check -q`
- Result: success.

## Files Modified
- `build.rs`
- `SECURITY_FIX_REPORT.md`

## Notes
- `pr-comment.md` was already modified in the working tree before this remediation and was not changed by the security fix work.
