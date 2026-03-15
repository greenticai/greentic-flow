# Security Fix Report

Date: 2026-03-15 (UTC)
Repository: `greentic-flow`

## Inputs Reviewed
- Dependabot alerts JSON: `0` alerts.
- Code scanning alerts JSON: `0` alerts.
- PR dependency vulnerabilities: `2` affected packages in `Cargo.lock`.

## PR Vulnerability Findings
1. `aws-lc-sys@0.37.1` (high)
   - GHSA-vw5v-4f2q-w9xf
   - GHSA-65p9-r9h6-22vj
   - GHSA-hfpc-8r3f-gw53
   - Advisory indicates patched version starts at `0.38.0`.
2. `wasmtime@41.0.3` (moderate)
   - GHSA-xjhv-v822-pf94
   - Advisory indicates patched version starts at `41.0.4`.

## Remediation Applied
Updated dependency constraints in `Cargo.toml` to avoid exact-pinning interface crates that can keep older vulnerable transitive graphs locked:
- `greentic-interfaces-host`: `=0.4.108` -> `0.4`
- `greentic-interfaces-wasmtime`: `=0.4.108` -> `0.4`

Rationale:
- This is the minimal safe source-level change that allows Cargo to resolve patched transitive versions instead of being constrained by exact pins.
- It reduces the likelihood of retaining vulnerable `wasmtime 41.0.3` in future lock resolution.

## CI Environment Limitation
A full lockfile remediation was not possible in this execution environment because crates.io index access is unavailable (offline DNS/network restriction), so `cargo update`/`cargo tree` dependency resolution against crates.io could not run.

## Required Follow-up (Networked CI Runner)
Run the following in a network-enabled CI step and commit the resulting `Cargo.lock` updates:

```bash
RUSTUP_TOOLCHAIN=stable cargo update
RUSTUP_TOOLCHAIN=stable cargo update -p wasmtime --precise 41.0.4
RUSTUP_TOOLCHAIN=stable cargo update -p aws-lc-sys --precise 0.38.0
RUSTUP_TOOLCHAIN=stable cargo audit
```

If Cargo reports a transitive constraint conflict for `aws-lc-sys 0.38.0`, upgrade the parent crate(s) that require `aws-lc-sys 0.37.x` (commonly via newer `rustls`/`rustls-webpki`/`quinn-proto` chain) and regenerate `Cargo.lock`.

## Files Changed
- `Cargo.toml`
- `SECURITY_FIX_REPORT.md`
