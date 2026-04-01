# Security Fix Report

## Scope
- Reviewed provided CodeQL and Dependabot alerts.
- Dependabot alerts in input: `0`.
- PR dependency vulnerabilities in input: `[]` (none reported).

## Fixes Applied

### 1) GitHub Actions code injection (CodeQL alert #73)
- File: `.github/workflows/codex-security-fix.yml`
- Fix:
  - Added an intermediate environment variable:
    - `PR_HEAD_REF: ${{ github.event.pull_request.head.ref }}`
  - Replaced inline expression interpolation in shell with shell variable usage:
    - from `PR_REF="refs/heads/${{ github.event.pull_request.head.ref }}"`
    - to `PR_REF="refs/heads/${PR_HEAD_REF}"`
- Security impact:
  - Removes direct interpolation of untrusted PR branch input inside a shell command context, matching GitHub hardening guidance.

### 2) SSRF hardening in CLI URL consumers (CodeQL alert #68 class)
- File: `src/bin/greentic-flow.rs`
- Fixes:
  - Added `validate_no_private_host_url(raw, label)` and `is_private_or_local_ip(ip)`.
  - Enforced URL validation before outbound requests in:
    - `fetch_remote_asset` (remote asset download path)
    - `load_frequent_components_catalog_from_location` (catalog HTTP load path)
    - `resolve_component_id_reference` (`--distributor-url` input)
- Validation now enforces:
  - HTTPS (except test-local HTTP helper path).
  - Host present.
  - No localhost/localdomain.
  - No private/local/reserved IP literals.
- Security impact:
  - Reduces risk of user-controlled endpoints targeting internal/local services.

### 3) Log injection hardening (CodeQL alerts #71, #69 class)
- Files:
  - `src/bin/greentic-flow.rs`
  - `tests/add_step_integration.rs`
- Fixes:
  - Added `sanitize_for_log` helper removing `\n` and `\r`.
  - Applied sanitization to user-influenced values before composing warning output in `report_empty_schema`.
  - Applied sanitization in test diagnostic output using env-driven component ID.
- Security impact:
  - Prevents newline-based forged log entries from unsanitized user-controlled values.

### 4) Path injection hardening (CodeQL alerts #14, #16, #17, #23, #67 class)
- Files:
  - `src/component_schema.rs`
  - `src/loader.rs`
  - `src/cache/disk.rs`
  - `build.rs`
- Fixes:
  - Added user path validation helpers in `src/component_schema.rs` and `src/loader.rs` to reject empty paths and parent-directory traversal (`..`) components before file reads.
  - Hardened cache prune path handling in `src/cache/disk.rs`:
    - Canonicalize artifacts directory once.
    - Canonicalize candidate artifact path and require it to remain under artifacts directory before metadata reads/deletions.
  - Added `OUT_DIR` component validation in `build.rs` to reject empty/traversal-like forms before write target construction.
- Security impact:
  - Adds explicit validation/containment checks around user-influenced and filesystem-derived paths.

## Unpinned Action Alerts (CodeQL `actions/unpinned-tag`)
- Alerts in provided list:
  - `openai/codex-action@v1`
  - `gittools/actions/gitversion/execute@v1`
  - `taiki-e/install-action@cargo-binstall`
  - `softprops/action-gh-release@v2`
- Status: **not pinned in this change set**.
- Reason:
  - This CI environment has no outbound DNS/network access (`github.com` resolution failed), so authoritative commit SHAs for each upstream action tag could not be retrieved/verified safely.
- Required follow-up:
  - Replace each tag with a verified full commit SHA from the upstream canonical repository.

## Validation Performed
- `cargo check --quiet` ✅
- `cargo test --quiet --test add_step_integration --no-run` ✅

## Files Modified
- `.github/workflows/codex-security-fix.yml`
- `build.rs`
- `src/bin/greentic-flow.rs`
- `src/cache/disk.rs`
- `src/component_schema.rs`
- `src/loader.rs`
- `tests/add_step_integration.rs`

## Notes
- Per instructions, I did not alter report/input artifact JSON files for remediation logic; pre-existing workspace changes in those artifact files were left untouched.
