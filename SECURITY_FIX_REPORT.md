# Security Fix Report

## Scope
Reviewed the provided CodeQL and Dependabot alert set and applied minimal source/workflow hardening fixes in this repository.

## Dependency Vulnerabilities (PR check)
- Input indicated `New PR Dependency Vulnerabilities: []`.
- No new dependency vulnerability entries were provided to remediate.

## Fixes Applied

### 1) Code scanning alert #72 (`actions/unpinned-tag`)
- File: `.github/workflows/codex-security-fix.yml`
- Change: pinned `openai/codex-action@v1` to immutable commit SHA:
  - `openai/codex-action@c25d10f3f498316d4b2496cc4c6dd58057a7b031 # v1`
- Security impact: removes mutable tag supply-chain risk for this third-party action.

### 2) Path-injection hardening for digest-based cache paths
- File: `src/bin/greentic-flow.rs`
- Changes:
  - Added `is_valid_sha256_digest` and `validate_component_digest`.
  - Enforced digest validation before `open_cached(...)` calls.
  - Hardened `cached_component_manifest_from_digest(...)` to reject invalid digest input before path construction.
- Security impact: prevents malformed user-influenced digest strings from reaching cache path lookups/open operations.

### 3) Path-injection hardening for disk cache key file naming
- File: `src/cache/disk.rs`
- Changes:
  - Added strict SHA-256 digest parser (`digest_hex`).
  - `paths_for(...)` now rejects invalid digests and normalizes file stem to `sha256_<64hex>`.
  - `is_valid_cache_stem(...)` now validates the normalized digest stem format.
- Security impact: prevents unsafe filename/path derivation from untrusted or malformed digest values.

## Alerts Reviewed But Not Reproduced in Current Files
The following alerts appear to reference code/workflow revisions that differ from the current repository state:
- #73 (`actions/code-injection/medium`) in `.github/workflows/codex-security-fix.yml` (line content now uses validated PR SHA flow, not branch ref interpolation).
- #11 (`actions/unpinned-tag`) in `.github/workflows/dev-publish.yml` (flagged `gittools/actions/...@v1`, not present now).
- #7 (`actions/unpinned-tag`) in `.github/workflows/nightly-wizard-e2e.yml` (flagged `taiki-e/install-action`, not present now).
- #12 (`actions/unpinned-tag`) in `.github/workflows/publish.yml` (flagged `softprops/action-gh-release`, not present now).
- #69 (`rust/log-injection`) in `tests/add_step_integration.rs` appears already mitigated by log sanitization helper.
- #71 (`rust/log-injection`) and #14/#16/#17/#23/#67/#68 likely include stale or conservative taint paths; direct hardening was still applied where user-controlled digest/path construction existed.

## Validation
- Ran: `cargo check -q`
- Result: success.

## Files Modified
- `.github/workflows/codex-security-fix.yml`
- `src/bin/greentic-flow.rs`
- `src/cache/disk.rs`
- `SECURITY_FIX_REPORT.md`
