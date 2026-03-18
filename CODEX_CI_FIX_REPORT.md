# CODEX CI Fix Report

## Summary
CI dependency/build failures were caused by transitive build scripts writing into read-only crate source locations under Cargo registry paths. I fixed this by vendoring the affected crates and patching resolution to local writable paths.

## Applied Fixes
1. Added local crate patch overrides in `Cargo.toml`:
- `greentic-interfaces = { path = "vendor/greentic-interfaces-0.4.106" }`
- `greentic-interfaces-wasmtime = { path = "vendor/greentic-interfaces-wasmtime-0.4.106" }`

2. Vendored crates into repository:
- `vendor/greentic-interfaces-0.4.106`
- `vendor/greentic-interfaces-wasmtime-0.4.106`

3. Fixed vendoring path behavior in `vendor/greentic-interfaces-0.4.106/src/lib.rs`:
- Updated `wit_root()` candidate priority to prefer crate-local `wit/` before workspace-level `../../wit`.
- This prevents empty/incorrect WIT discovery after vendoring and restores expected generated module set.

4. Regenerated lock metadata to include local patched sources (`Cargo.lock` updated).

## Verification
I ran the required commands successfully in this environment (offline mode due sandbox DNS/index restrictions):

- `cargo fmt --all -- --check` ✅
- `cargo clippy --workspace --all-targets -- -D warnings` ✅ (via `--offline`)
- `cargo build --workspace --all-features` ✅ (via `--offline`)
- `cargo test --workspace --all-features -- --nocapture` ✅ (via `--offline`)

All tests pass; no test files were modified.

## Remaining Blockers
No code blockers remain.

Environment note: this sandbox cannot reliably access `index.crates.io` (DNS/network restricted), so direct online resolution failed until commands were run with `--offline`. In normal CI with network access, these dependency patches should resolve the original read-only build-script failures.
