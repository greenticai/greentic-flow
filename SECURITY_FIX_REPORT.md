# Security Fix Report

## Scope
- Reviewed all provided CodeQL alerts and PR dependency vulnerability input.
- Checked for new PR dependency vulnerabilities from provided input: `[]` (none).
- Applied minimal, direct fixes in reported files where active risk paths remained.

## Fixes Applied

### 1) Code injection in workflow (Alert #73)
- File: `.github/workflows/codex-security-fix.yml`
- Change:
  - Moved PR head ref usage into step `env` (`PR_HEAD_REF`) and consumed it via shell variable (`FIX_BRANCH="${PR_HEAD_REF:-}"`).
  - Retained strict branch-name validation regex and traversal checks before push.
- Security impact:
  - Avoids direct expression interpolation in shell logic and keeps untrusted ref constrained before use.

### 2) Path injection hardening (Alerts #14, #16, #17, #23, #67)
- Files:
  - `src/path_safety.rs`
  - `src/loader.rs`
  - `src/component_schema.rs`
  - `src/bin/greentic-flow.rs`
  - `src/cache/disk.rs`
- Changes:
  - Strengthened `normalize_under_root` to canonicalize root and enforce containment for both relative and absolute candidate paths.
  - Updated loaders/schema resolution to use root-constrained normalization uniformly (including absolute inputs).
  - Hardened sidecar local-path resolution to require paths remain under validated flow directory.
  - Added cache prune guardrails so discovered metadata files are canonicalized and required to remain under cache artifacts root.
- Security impact:
  - Reduces traversal/escape risk via absolute paths, symlink tricks, or unsafe sidecar-relative resolution.

### 3) SSRF hardening (Alert #68)
- File: `src/bin/greentic-flow.rs`
- Change:
  - Removed user-parameterized URL argument from `load_frequent_components_catalog_from_url` and now fetches only the internally computed latest catalog URL.
  - Existing https + allowlisted-host checks remain in place.
- Security impact:
  - Eliminates user-controlled request URL flow into HTTP client for this path.

### 4) Build script path hardening (Alert #14 context in `build.rs`)
- File: `build.rs`
- Change:
  - Added `trusted_out_dir()` validation to ensure canonical `OUT_DIR` stays under Cargo target directory before writing output artifacts.
- Security impact:
  - Constrains env-influenced output path usage during build.

## Alerts Already Remediated / Not Reproduced in Current Tree

### Unpinned Action alerts (#72, #11, #7, #12)
- Current workflow files already do not match the previously reported vulnerable lines/refs:
  - `.github/workflows/codex-security-fix.yml` already pins `openai/codex-action` to full SHA.
  - `.github/workflows/dev-publish.yml` no longer uses the flagged `gittools/actions/gitversion/execute@v1` step.
  - `.github/workflows/nightly-wizard-e2e.yml` no longer uses `taiki-e/install-action@cargo-binstall`.
  - `.github/workflows/publish.yml` no longer uses `softprops/action-gh-release@v2`.
- Disposition: likely stale alerts pending new scan lifecycle.

### Test log injection alert (#69)
- File `tests/add_step_integration.rs` already contains newline/carriage-return sanitization before logging in current tree.
- Disposition: likely stale after code scan refresh.

## Dependency Vulnerabilities (PR)
- Input `New PR Dependency Vulnerabilities` was empty (`[]`).
- No dependency remediation required from provided PR vulnerability data.

## Validation
- Ran: `cargo check`
- Result: success.

## Notes
- Existing unrelated local modification `pr-comment.md` was left untouched.
