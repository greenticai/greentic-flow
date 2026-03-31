# Security Fix Report

## Scope
- Reviewed all provided CodeQL alerts and PR dependency vulnerability input.
- Checked current repository state for direct fixes in reported files.
- Applied minimal hardening changes without touching alert/input artifacts.

## PR dependency vulnerability check
- Input `New PR Dependency Vulnerabilities` is `[]`.
- No new dependency vulnerabilities were introduced in dependency files in this run.

## Fixes applied

### 1) Code injection in workflow (`actions/code-injection/medium`, alert #73)
- File: `.github/workflows/codex-security-fix.yml`
- Change: removed direct expression-derived `PR_HEAD_REF` environment assignment and switched branch source to `GITHUB_HEAD_REF` inside shell runtime.
- Additional guard remains in place: strict branch-name allowlist regex and rejection of unsafe prefixes/`..`.
- Security impact: reduces risk of untrusted PR ref values being interpreted in expression-to-shell contexts.

### 2) Path injection hardening in build path handling (`rust/path-injection`, alert #14)
- File: `build.rs`
- Change: canonicalize computed `target_root` when present; if not yet existing, require absolute path and reject any parent traversal components.
- Security impact: tightens trust boundary around env-derived target directories before path containment checks.

### 3) Path injection hardening in disk cache prune flow (`rust/path-injection`, alert #16)
- File: `src/cache/disk.rs`
- Change: derive artifact path from canonical metadata path (`with_extension("cwasm")`) and enforce artifact path remains under canonical cache root.
- Security impact: avoids constructing artifact paths from potentially attacker-influenced filename fragments.

### 4) Path injection hardening in component manifest resolution (`rust/path-injection`, alert #67)
- File: `src/bin/greentic-flow.rs`
- Changes:
  - enforce `validate_component_ref(r#ref)?` for OCI/Repo/Store paths before cache resolution.
  - canonicalize manifest path before reading defaults and require it to be a file.
- Security impact: ensures remote component references are validated consistently and manifest reads resolve to canonical file targets.

## Alert analysis notes (no additional code changes required)
- `actions/unpinned-tag` alerts #11, #7, #12 and #72 reference workflow steps/refs that are not present in the current checked-out files (the currently referenced `openai/codex-action` entry is pinned to a full commit SHA).
- `rust/log-injection` alert #69 in `tests/add_step_integration.rs` is already mitigated by log sanitization helper usage.
- `rust/log-injection` alert #71 and `rust/path-injection` alerts #17/#23 map to code paths that already include path/ref validation in current branch; no additional low-risk fix was necessary in those specific files beyond changes listed above.
- `rust/request-forgery` alert #68 points to code that, in current branch, no longer performs user-controlled URL fetch at the reported location; URL fetch path is constrained to HTTPS + allowlisted hosts.

## Validation
- Ran: `cargo check --all-targets`
- Result: success

