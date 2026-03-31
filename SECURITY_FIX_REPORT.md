# Security Fix Report

## Scope
- Reviewed the provided security alerts (Dependabot + CodeQL).
- PR dependency vulnerability input was empty (`[]`).
- Checked for newly introduced dependency-file vulnerabilities in this run: no dependency file changes were made.

## Fixes Applied

1. `actions/code-injection/medium` (alert #73)
- File: `.github/workflows/codex-security-fix.yml`
- Changes:
  - Checkout now uses immutable PR head SHA (`github.event.pull_request.head.sha`) instead of branch ref text.
  - Removed direct `${{ github.event.pull_request.head.ref }}` env interpolation in the push step.
  - Pull request branch name is now read from `$GITHUB_EVENT_PATH` and strictly validated (`^[A-Za-z0-9._/-]+$`, rejects leading `-` and `..`) before `git push`.
- Result: reduced command-injection risk from untrusted PR branch names.

2. `rust/request-forgery` (alert #68)
- File: `src/bin/greentic-flow.rs`
- Changes:
  - Split catalog loading into:
    - `load_frequent_components_catalog_from_file_location` (local file only)
    - `load_frequent_components_catalog_from_url` (remote URL path)
  - `GREENTIC_FLOW_FREQUENT_COMPONENTS_URL` override now only loads local files; it no longer triggers arbitrary remote fetches.
  - Removed environment override from `frequent_components_latest_url`; remote fetch now uses repository-defined latest URL path.
- Result: removed user-controlled remote URL request path that triggered SSRF.

3. `rust/path-injection` (alerts #67, #23)
- File: `src/bin/greentic-flow.rs`
- Changes:
  - Removed unsafe fallback to relative `component.manifest.json` when artifact parent resolution fails.
  - Canonicalized resolved artifact paths before deriving parent manifest path.
  - Return explicit errors when parent path cannot be derived.
- Result: prevents fallback to attacker-influenced working-directory paths.

4. `rust/path-injection` (alert #16)
- File: `src/cache/disk.rs`
- Changes:
  - During prune scan, metadata read path is now rebuilt from validated cache stem (`artifacts_dir/<stem>.json`) instead of using raw `DirEntry::path()`.
- Result: tighter path derivation from validated identifiers.

5. `rust/path-injection` (alert #14)
- File: `build.rs`
- Changes:
  - Replaced runtime manifest file read with compile-time `include_str!("frequent-components.json")`.
- Result: removes build-time read from env-influenced manifest path.

6. Additional path hardening in reported files
- File: `src/component_schema.rs`
  - Enforced `component.manifest.json` filename and explicit `is_file()` check before read.
- File: `src/loader.rs`
  - Added explicit `is_file()` check after canonicalization and before read.

## Alert State Notes
- `actions/unpinned-tag` alerts #11, #7, #12 reference action uses not present in current workflow files.
- `actions/unpinned-tag` alert #72 remains for `openai/codex-action@v1` in `.github/workflows/codex-security-fix.yml`.
  - Could not pin to a full commit SHA in this environment because outbound network resolution to GitHub is unavailable.

## Validation
- `cargo fmt`
- `cargo test -q --no-run`

Both commands completed successfully.
