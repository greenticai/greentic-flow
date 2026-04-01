# SECURITY_FIX_REPORT

## Scope
- Reviewed provided security input.
- Dependabot alerts: none.
- Code scanning alerts: 6 open `rust/path-injection` alerts in `src/loader.rs` (lines 22, 44, 45, 48, 73, 107 in the reported scan snapshot).

## Root Cause
Path-based file operations in `src/loader.rs` accepted user-influenced or environment-influenced paths without an explicit allowlist boundary. CodeQL flagged these as CWE-22/23/36/73/99 path injection risks.

## Remediations Applied

### 1) Added allowlist path guard for loader file access
- File: `src/loader.rs`
- Added `is_allowed_absolute_path(path: &Path) -> io::Result<bool>`.
- Policy: resolved path must be under one of:
  - canonical current working directory, or
  - canonical system temp directory.

### 2) Hardened user path canonicalization
- File: `src/loader.rs`
- Updated `canonicalize_user_path` to:
  - canonicalize input,
  - require a regular file,
  - reject paths outside allowlisted roots with `PermissionDenied`.

### 3) Hardened schema path resolution for absolute schema inputs
- File: `src/loader.rs`
- Updated `load_ygtc_from_str_with_source` absolute-path branch to reuse `canonicalize_user_path`.
- Result: absolute schema paths now require allowlisted roots.

### 4) Hardened temp schema file path construction
- File: `src/loader.rs`
- Replaced direct `temp_dir` path assembly with `trusted_temp_schema_path()`:
  - canonicalizes temp root,
  - verifies trusted root policy,
  - builds deterministic schema filename only after trust check.
- `schema_file_valid` now rejects non-allowlisted paths before attempting reads.

## Regression Test Added
- File: `tests/load_err.rs`
- Added unix-only test `absolute_schema_path_outside_allowlist_is_rejected`.
- Verifies absolute schema paths outside allowed roots are rejected.

## Validation Status
- Could not run cargo tests in this CI sandbox due rustup filesystem restrictions:
  - error: `could not create temp file /home/runner/.rustup/tmp/...: Read-only file system (os error 30)`
- Static review confirms path guard is now enforced at all flagged loader path handling points.

## Files Changed
- `src/loader.rs`
- `tests/load_err.rs`
- `SECURITY_FIX_REPORT.md`
