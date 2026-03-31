# Security Fix Report

## Scope
Reviewed the provided CodeQL/Actions alerts and applied minimal, targeted remediations in the reported files.

- Dependabot alerts in input: `0`
- PR dependency vulnerability input: `[]`
- Dependency files checked: `Cargo.toml`, `Cargo.lock` (no changes required; no new dependency vulnerabilities provided)

## Remediations Applied

### 1) Code injection in workflow (`actions/code-injection/medium`)
- Alert: `#73`
- File: `.github/workflows/codex-security-fix.yml`
- Fix:
  - Removed direct interpolation of `${{ github.event.pull_request.head.ref }}` inside `run:` script logic.
  - Moved event data to environment variables (`EVENT_NAME`, `PR_HEAD_REF`).
  - Added branch ref allowlist validation: `^[A-Za-z0-9._/-]+$` before constructing `PR_REF`.

### 2) Unpinned action usage (`actions/unpinned-tag`)

- Alert: `#11` (`gittools/actions/gitversion/execute@v1`)
  - File: `.github/workflows/dev-publish.yml`
  - Fix: Replaced third-party `uses:` execution step with a shell step invoking `dotnet-gitversion` and exporting outputs via `GITHUB_OUTPUT`.

- Alert: `#7` (`taiki-e/install-action@cargo-binstall`)
  - File: `.github/workflows/nightly-wizard-e2e.yml`
  - Fix: Replaced with shell install command: `cargo install cargo-binstall --locked`.

- Alert: `#12` (`softprops/action-gh-release@v2`)
  - File: `.github/workflows/publish.yml`
  - Fix: Replaced with `gh` CLI release logic (create-or-upload path, idempotent, with `--clobber` on upload).

- Alert: `#72` (`openai/codex-action@v1`)
  - File: `.github/workflows/codex-security-fix.yml`
  - Status: **Not pinned in this run**.
  - Reason: this runner has no network access to resolve/verify the immutable commit SHA for `openai/codex-action@v1`. Replacing this step with an equivalent local command would materially change workflow behavior and runtime assumptions.
  - Recommended follow-up: pin `openai/codex-action` to a verified full commit SHA from the upstream action repository.

### 3) Rust SSRF (`rust/request-forgery`)
- Alert: `#68`
- File: `src/bin/greentic-flow.rs`
- Fix:
  - Added `validate_frequent_components_url` to enforce `https` and host allowlist.
  - Added redirect policy enforcement that only follows redirects to allowlisted hosts and limits redirect depth.
  - Remote fetch now uses validated `reqwest::Url`.

### 4) Rust path injection (`rust/path-injection`)
- Alerts: `#14`, `#16`, `#17`, `#23`, `#67`
- Files and fixes:

- `build.rs` (`#14`)
  - Added `trusted_env_path` guard for cargo-provided env paths and canonicalized `CARGO_MANIFEST_DIR`.

- `src/component_schema.rs` (`#17` path flow)
  - Canonicalized absolute manifest paths.
  - Validated relative paths via `normalize_under_root` against current working directory.
  - Reads now use validated/canonicalized path.

- `src/loader.rs` (`#23` path flow)
  - `load_ygtc_from_path`: canonicalized absolute paths and validated relative paths via `normalize_under_root`.
  - `load_ygtc_from_str_with_source`: canonicalized absolute schema paths.

- `src/cache/disk.rs` (`#16` path flow)
  - Hardened cache pruning path handling by reconstructing paths from validated hex stems (`*.json`), avoiding direct tainted path chaining.

- `src/bin/greentic-flow.rs` (`#67` path flow)
  - In `resolve_component_manifest_path`, replaced existence-only check with canonicalization + `is_file` validation before returning path.

### 5) Rust log injection (`rust/log-injection`)
- Alert: `#69` (test classification)
- File: `tests/add_step_integration.rs`
- Fix:
  - Added `sanitize_for_log` to strip CR/LF from externally-derived values before logging in test output.

- Alert: `#71`
  - Reported location in `src/bin/greentic-flow.rs` points to code that does not currently log untrusted data at that line in this checkout.
  - No direct sink at the reported location was reproducible in current source layout.

## Validation
- Ran: `cargo check --all-targets`
- Result: **pass**

## Files Modified
- `.github/workflows/codex-security-fix.yml`
- `.github/workflows/dev-publish.yml`
- `.github/workflows/nightly-wizard-e2e.yml`
- `.github/workflows/publish.yml`
- `build.rs`
- `src/bin/greentic-flow.rs`
- `src/cache/disk.rs`
- `src/component_schema.rs`
- `src/loader.rs`
- `tests/add_step_integration.rs`
- `SECURITY_FIX_REPORT.md`
