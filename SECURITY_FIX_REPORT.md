# Security Fix Report

## Scope
- Reviewed all provided CodeQL alerts and PR dependency vulnerability input.
- PR dependency vulnerabilities provided: `[]` (none).
- New dependency-file vulnerabilities introduced in this change set: none detected.

## Fixes Applied

### 1) Code injection in GitHub Actions (`actions/code-injection/medium`, alert #73)
- File: `.github/workflows/codex-security-fix.yml`
- Fix:
  - Removed direct expression interpolation inside shell conditionals and command construction.
  - Introduced step environment variables:
    - `EVENT_NAME: ${{ github.event_name }}`
    - `PR_HEAD_REF: ${{ github.event.pull_request.head.ref }}`
  - Updated shell usage to native variable expansion (`$EVENT_NAME`, `$PR_HEAD_REF`).
- Security effect: prevents GitHub expression values from being interpreted directly in shell code contexts.

### 2) Unpinned third-party action usage (`actions/unpinned-tag`)
- File: `.github/workflows/nightly-wizard-e2e.yml` (alert #7)
  - Replaced `taiki-e/install-action@cargo-binstall` with `run: cargo install cargo-binstall --locked`.
- File: `.github/workflows/publish.yml` (alert #12)
  - Replaced `softprops/action-gh-release@v2` with `gh release` CLI commands (`view/create/upload`).
- File: `.github/workflows/dev-publish.yml` (alert #11)
  - Replaced `gittools/actions/gitversion/setup@v1` and `gittools/actions/gitversion/execute@v1` with an in-workflow shell-based version computation.

### 3) Path injection hardening (`rust/path-injection`)
- File: `build.rs` (alert #14)
  - Canonicalized `OUT_DIR` and enforced that it resolves under the target root (`CARGO_TARGET_DIR` or `manifest_dir/target`) before write.
- File: `src/loader.rs` (alert #23 related loader path handling)
  - Replaced lightweight path-component checks with canonicalized file-path validation (`canonicalize_user_path`) before read.
  - Reads now use canonicalized paths and validated file type.
- File: `src/component_schema.rs` (alert #17 related manifest path handling)
  - Same canonicalized file-path validation pattern before reading manifest.
- File: `src/cache/disk.rs` (alert #16 related cache path construction)
  - Hardened digest-to-filename conversion to strict safe filename characters (`[A-Za-z0-9_-]`), replacing unsafe characters and preventing path separator traversal.

### 4) SSRF/request-forgery hardening (`rust/request-forgery`, alert #68)
- File: `src/bin/greentic-flow.rs`
- Fix:
  - Replaced `validate_no_private_host_url` with `parse_and_validate_outbound_url` returning a parsed `reqwest::Url`.
  - Added URL hardening checks:
    - scheme enforcement (`https`, with test-only localhost exception already present),
    - disallow URL userinfo,
    - disallow URL fragments,
    - existing private/local host rejection retained.
  - Updated outbound request call sites to use validated URL objects:
    - remote asset download,
    - frequent component catalog download,
    - distributor base URL for client config.

## Alerts Not Fully Remediated In This Pass
- `actions/unpinned-tag` on `.github/workflows/codex-security-fix.yml` (`openai/codex-action@v1`, alert #72):
  - Remains as a tag reference because pinning to an exact commit SHA requires upstream tag resolution from GitHub, which is unavailable in this offline CI environment.
  - Recommended follow-up: pin `openai/codex-action` to a verified full commit SHA.

## Validation
- Ran `cargo fmt --all`.
- Ran `cargo check -q` successfully after changes.

## Files Changed
- `.github/workflows/codex-security-fix.yml`
- `.github/workflows/dev-publish.yml`
- `.github/workflows/nightly-wizard-e2e.yml`
- `.github/workflows/publish.yml`
- `build.rs`
- `src/bin/greentic-flow.rs`
- `src/cache/disk.rs`
- `src/component_schema.rs`
- `src/loader.rs`
- `SECURITY_FIX_REPORT.md`
