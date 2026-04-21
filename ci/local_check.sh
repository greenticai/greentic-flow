#!/usr/bin/env bash
# Usage:
#   LOCAL_CHECK_ONLINE=1 LOCAL_CHECK_STRICT=1 ci/local_check.sh
# Defaults: offline, non-strict.

set -euo pipefail

LOCAL_CHECK_ONLINE=${LOCAL_CHECK_ONLINE:-0}
LOCAL_CHECK_STRICT=${LOCAL_CHECK_STRICT:-0}
LOCAL_CHECK_VERBOSE=${LOCAL_CHECK_VERBOSE:-0}
LOCAL_CHECK_ALLOW_SKIP=${LOCAL_CHECK_ALLOW_SKIP:-0}
LOCAL_CHECK_RUST_MM=${LOCAL_CHECK_RUST_MM:-}
LOCAL_CHECK_SCHEMA_REF=${LOCAL_CHECK_SCHEMA_REF:-main}
LOCAL_CHECK_JOBS=${LOCAL_CHECK_JOBS:-1}
LOCAL_CHECK_MIN_FREE_MB=${LOCAL_CHECK_MIN_FREE_MB:-2048}
SKIPPED_REQUIRED=0
declare -a REQUIRED_SKIP_REASONS=()

if [[ "${LOCAL_CHECK_VERBOSE}" == "1" ]]; then
  set -x
fi

need() {
  command -v "$1" >/dev/null 2>&1
}

step() {
  echo ""
  echo "▶ $*"
}

available_mb() {
  df -Pm . | awk 'NR==2 {print $4}'
}

ensure_free_space_mb() {
  local required_mb=$1
  local available
  available="$(available_mb)"
  if [[ -z "${available}" ]]; then
    skip_step "unable to determine free disk space" 1
    return
  fi
  if (( available < required_mb )); then
    skip_step "insufficient free disk space: need at least ${required_mb}MB, found ${available}MB (set LOCAL_CHECK_MIN_FREE_MB to override)" 1
  fi
}

skip_step() {
  local reason=$1
  local required=${2:-0}
  if [[ "${required}" == "1" ]]; then
    SKIPPED_REQUIRED=1
    REQUIRED_SKIP_REASONS+=("${reason}")
  fi
  if [[ "${LOCAL_CHECK_STRICT}" == "1" ]]; then
    echo "[FAIL] ${reason}"
    exit 1
  else
    echo "[skip] ${reason}"
  fi
}

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"

if [[ -z "${LOCAL_CHECK_RUST_MM}" ]]; then
  if [[ -f rust-toolchain.toml ]]; then
    LOCAL_CHECK_RUST_MM="$(sed -nE 's/^[[:space:]]*channel[[:space:]]*=[[:space:]]*"([0-9]+\.[0-9]+)(\.[0-9]+)?".*/\1/p' rust-toolchain.toml | head -n 1)"
  fi
fi
if [[ -z "${LOCAL_CHECK_RUST_MM}" ]]; then
  LOCAL_CHECK_RUST_MM="$(sed -nE 's/^[[:space:]]*rust-version[[:space:]]*=[[:space:]]*"([0-9]+\.[0-9]+)(\.[0-9]+)?".*/\1/p' Cargo.toml | head -n 1)"
fi

step "Toolchain versions"
if need rustc; then
  rustc --version
  rustc_mm="$(rustc -V | awk '{print $2}' | cut -d. -f1,2)"
  if [[ -n "${LOCAL_CHECK_RUST_MM}" && "${rustc_mm}" != "${LOCAL_CHECK_RUST_MM}" ]]; then
    skip_step "rustc ${LOCAL_CHECK_RUST_MM}.x required (found $(rustc -V | awk '{print $2}'))" 1
  fi
else
  skip_step "rustc not found" 1
fi
if need cargo; then
  cargo --version
else
  skip_step "cargo not found" 1
fi
echo "cargo jobs: ${LOCAL_CHECK_JOBS}"
step "Check free disk space"
ensure_free_space_mb "${LOCAL_CHECK_MIN_FREE_MB}"

step "Verify canonical greentic component WIT is not vendored"
if [[ -x ci/check_no_duplicate_canonical_wit.sh ]]; then
  ci/check_no_duplicate_canonical_wit.sh
else
  skip_step "ci/check_no_duplicate_canonical_wit.sh missing or not executable" 1
fi

step "Verify component-wizard ABI is not used in src/tests"
if [[ -x ci/check_no_component_wizard_usage.sh ]]; then
  ci/check_no_component_wizard_usage.sh
else
  skip_step "ci/check_no_component_wizard_usage.sh missing or not executable" 1
fi

step "Verify greentic_interfaces bindings::* is not used in downstream code/docs"
if [[ -x ci/check_no_greentic_interfaces_bindings_usage.sh ]]; then
  ci/check_no_greentic_interfaces_bindings_usage.sh
else
  skip_step "ci/check_no_greentic_interfaces_bindings_usage.sh missing or not executable" 1
fi

step "cargo fmt --all -- --check"
if need cargo; then
  cargo fmt --all -- --check
else
  skip_step "cargo fmt requires cargo"
fi

step "cargo clippy --workspace --all-targets --all-features -D warnings"
if need cargo; then
  cargo clippy -j "${LOCAL_CHECK_JOBS}" --workspace --all-targets --all-features -- -D warnings
else
  skip_step "cargo clippy requires cargo"
fi

step "cargo build --workspace --locked --all-features"
if need cargo; then
  cargo build -j "${LOCAL_CHECK_JOBS}" --workspace --locked --all-features
else
  skip_step "cargo build requires cargo"
fi

step "cargo test --workspace --all-features"
if need cargo; then
  cargo test -j "${LOCAL_CHECK_JOBS}" --workspace --all-features
else
  skip_step "cargo test requires cargo"
fi

step "greentic-flow doctor --json smoke test"
if ! need python3; then
  skip_step "python3 required for smoke test" 1
elif ! need cargo && [[ ! -x target/debug/greentic-flow ]]; then
  skip_step "cargo required to build greentic-flow" 1
else
  if [[ ! -x target/debug/greentic-flow ]]; then
    cargo build -j "${LOCAL_CHECK_JOBS}" --quiet --bin greentic-flow
  fi
  ./target/debug/greentic-flow doctor --json tests/data/flow_ok.ygtc | python3 -c 'import json,sys; data=json.load(sys.stdin); assert data.get("ok") is True, data'
fi

step "Verify published schema \$id"
if [[ "${LOCAL_CHECK_ONLINE}" != "1" ]]; then
  skip_step "online schema check disabled (set LOCAL_CHECK_ONLINE=1)" 0
elif ! need curl; then
  skip_step "curl required for schema check" 1
elif ! need python3; then
  skip_step "python3 required for schema check" 1
else
  url="https://raw.githubusercontent.com/greentic-ai/greentic-flow/refs/heads/${LOCAL_CHECK_SCHEMA_REF}/schemas/ygtc.flow.schema.json"
  tmp_schema="$(mktemp)"
  if ! curl -sSf "${url}" -o "${tmp_schema}"; then
    skip_step "schema fetch failed (offline?). Skipping schema parity check." 0
  else
    TMP_SCHEMA="${tmp_schema}" python3 - <<'PY'
import json, os, sys
published = json.load(open(os.environ["TMP_SCHEMA"]))
local = json.load(open("schemas/ygtc.flow.schema.json"))
if published.get("$id") != local.get("$id"):
    raise SystemExit(f"Schema $id mismatch: remote={published.get('$id')} local={local.get('$id')}")
PY
  fi
  rm -f "${tmp_schema}"
fi

if [[ "${SKIPPED_REQUIRED}" == "1" && "${LOCAL_CHECK_ALLOW_SKIP}" != "1" ]]; then
  echo ""
  echo "[FAIL] Required CI steps were skipped:"
  for reason in "${REQUIRED_SKIP_REASONS[@]}"; do
    echo "  - ${reason}"
  done
  echo "Re-run with the required tools installed, or set LOCAL_CHECK_ALLOW_SKIP=1 to override."
  exit 2
fi

echo ""
echo "✅ local_check completed"
