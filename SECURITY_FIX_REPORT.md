# Security Fix Report

## Scope
Reviewed the CodeQL alerts provided for `src/bin/greentic-flow.rs` and applied minimal, targeted hardening changes for the flagged classes:
- `rust/path-injection`
- `rust/request-forgery`
- `rust/log-injection` (no direct vulnerable sink remained at the currently referenced offsets; existing log sanitization was already present)

## Remediations Applied

### 1) Path traversal / path injection hardening for wizard plan execution
Added strict validation for plan-provided flow paths before any filesystem join/use:
- New helpers:
  - `safe_plan_flow_rel_path(flow, label)`
  - `safe_pack_join_from_plan(pack_dir, flow, label)`
- These enforce relative, non-escaping paths via existing `ensure_safe_pack_relative_path`.

Updated call sites to use validated paths:
- `schema_for_wizard_plan_action` (flow schema/question resolution path)
- `sync_pack_assets_for_flow`
- `execute_add_flow_plan_action`
- `execute_edit_flow_summary_plan_action`
- `execute_delete_flow_plan_action`
- `execute_add_step_plan_action`
- `execute_update_step_plan_action`
- `execute_delete_step_plan_action`

### 2) Plan local-wasm path restriction
Hardened `resolve_plan_local_wasm`:
- Signature changed to return `Result<PathBuf>`.
- Rejects absolute paths.
- Validates relative path with `ensure_safe_pack_relative_path`.
- All plan action callers now handle validation errors via `.transpose()?`.

This prevents plan-driven arbitrary filesystem access for local wasm references.

### 3) SSRF hardening for outbound URL fetches
Strengthened outbound URL validation in `parse_and_validate_outbound_url`:
- Added DNS resolution check `validate_public_dns_targets(url, label)`.
- Rejects hosts resolving to private/local/link-local/loopback/etc. IPs.
- Ensures host resolves to at least one address.

This complements existing checks (scheme, userinfo, fragment, direct host/IP checks) and mitigates DNS-based SSRF bypasses.

## Files Changed
- `src/bin/greentic-flow.rs`

## Verification
Attempted to run:
- `cargo check --bin greentic-flow`

CI sandbox prevented rustup temp writes (`/home/runner/.rustup/...` read-only), so a full compile check could not be completed in this environment.

## Notes
- Existing log sanitization helper (`sanitize_for_log`) remains in place and is used for schema-warning/error emission paths.
- Changes were kept minimal and focused on tainted path/URL handling at execution boundaries.
