#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FLOW_BIN="${FLOW_BIN:-$ROOT_DIR/target/debug/greentic-flow}"
CATALOG_PATH="${CATALOG_PATH:-$ROOT_DIR/frequent-components.json}"
FIXTURES_DIR="${FIXTURES_DIR:-$ROOT_DIR/tests/e2e/frequent-components}"
TMP_ROOT="${TMPDIR:-/tmp}/greentic-flow-nightly-wizard-e2e"

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing required command: $1" >&2
    exit 1
  fi
}

require_cmd python3
require_cmd greentic-pack

if [[ ! -x "$FLOW_BIN" ]]; then
  echo "greentic-flow binary not found at $FLOW_BIN" >&2
  exit 1
fi

if [[ ! -f "$CATALOG_PATH" ]]; then
  echo "frequent component catalog not found at $CATALOG_PATH" >&2
  exit 1
fi

rm -rf "$TMP_ROOT"
mkdir -p "$TMP_ROOT"
trap 'rm -rf "$TMP_ROOT"' EXIT

log() {
  printf '==> %s\n' "$*"
}

flow_doctor_assert_ok() {
  local flow_path="$1"
  local doctor_json
  doctor_json="$("$FLOW_BIN" doctor --json "$flow_path")"
  python3 - "$doctor_json" <<'PY'
import json
import sys

payload = json.loads(sys.argv[1])
if not payload.get("ok", False):
    raise SystemExit(f"doctor failed: {json.dumps(payload, indent=2)}")
PY
}

create_pack_with_main_flow() {
  local slug="$1"
  local root="$TMP_ROOT/$slug"
  local pack_dir="$root/pack"
  local flow_path="$pack_dir/flows/global/messaging/main.ygtc"
  mkdir -p "$root"
  greentic-pack new --dir "$pack_dir" "ai.greentic.nightly.$slug" >/dev/null
  "$FLOW_BIN" new \
    --flow "$flow_path" \
    --id main \
    --type messaging \
    --force >/dev/null
  printf '%s\n' "$flow_path"
}

component_rows() {
  python3 - "$CATALOG_PATH" <<'PY'
import json
import sys

catalog = json.load(open(sys.argv[1], "r", encoding="utf-8"))
for component in catalog["components"]:
    print(f'{component["id"]}\t{component["component_ref"]}')
PY
}

ensure_answers_fixture() {
  local component_id="$1"
  local mode_name="$2"
  local fixture_path="$FIXTURES_DIR/$component_id/$mode_name.answers.json"
  [[ -f "$fixture_path" ]] || {
    echo "missing answers fixture: $fixture_path" >&2
    exit 1
  }
  printf '%s\n' "$fixture_path"
}

materialize_answers_fixture() {
  local component_id="$1"
  local mode_name="$2"
  local fixture_path="$3"
  local out_dir="$TMP_ROOT/materialized-fixtures/$component_id"
  local out_path="$out_dir/$mode_name.answers.json"
  mkdir -p "$out_dir"
  sed "s|__ROOT_DIR__|$ROOT_DIR|g" "$fixture_path" >"$out_path"
  printf '%s\n' "$out_path"
}

answers_artifact_path() {
  local flow_path="$1"
  local node_id="$2"
  local mode_name="$3"
  local flow_dir
  flow_dir="$(dirname "$flow_path")"
  printf '%s\n' "$flow_dir/answers/main/$node_id/$mode_name.answers.json"
}

assert_flow_contains_component() {
  local flow_path="$1"
  local node_id="$2"
  local component_ref="$3"
  python3 - "$flow_path" "$node_id" "$component_ref" <<'PY'
import json
import pathlib
import sys

flow_text = pathlib.Path(sys.argv[1]).read_text(encoding="utf-8")
node_id = sys.argv[2]
component_ref = sys.argv[3]
if node_id not in flow_text:
    raise SystemExit(f"flow is missing node id {node_id}")

sidecar_path = pathlib.Path(sys.argv[1]).with_suffix(".ygtc.resolve.json")
sidecar = json.loads(sidecar_path.read_text(encoding="utf-8"))
nodes = sidecar.get("nodes", {})
if node_id not in nodes:
    raise SystemExit(f"sidecar is missing node {node_id}")
source = nodes[node_id].get("source", {})
actual = source.get("ref") or source.get("path")
if actual != component_ref:
    raise SystemExit(
        f"unexpected component source for {node_id}: expected {component_ref!r}, got {actual!r}"
    )
PY
}

run_add_step_case() {
  local component_id="$1"
  local component_ref="$2"
  local mode_name="$3"
  local interactive_flag="$4"
  local run_kind
  local wizard_mode
  local flow_path
  local fixture_path
  local node_id
  local artifact_path

  if [[ "$mode_name" == "personalised" ]]; then
    wizard_mode="setup"
  else
    wizard_mode="default"
  fi

  run_kind="answers-file"
  if [[ "$interactive_flag" == "interactive" ]]; then
    run_kind="interactive"
  fi

  flow_path="$(create_pack_with_main_flow "${component_id}-${mode_name}-${run_kind}")"
  fixture_path="$(ensure_answers_fixture "$component_id" "$mode_name")"
  fixture_path="$(materialize_answers_fixture "$component_id" "$mode_name" "$fixture_path")"
  node_id="${component_id//-/_}_${mode_name}_${run_kind}"
  artifact_path="$(answers_artifact_path "$flow_path" "$node_id" "$wizard_mode")"

  log "add-step $component_id ($mode_name, $run_kind)"

  if [[ "$interactive_flag" == "interactive" ]]; then
    "$FLOW_BIN" add-step \
      --flow "$flow_path" \
      --node-id "$node_id" \
      --component "$component_ref" \
      --wizard-mode "$wizard_mode" \
      --answers-file "$fixture_path" \
      --interactive \
      --routing-out </dev/null
  else
    "$FLOW_BIN" add-step \
      --flow "$flow_path" \
      --node-id "$node_id" \
      --component "$component_ref" \
      --wizard-mode "$wizard_mode" \
      --answers-file "$fixture_path" \
      --routing-out
  fi

  assert_flow_contains_component "$flow_path" "$node_id" "$component_ref"
  [[ -f "$artifact_path" ]] || {
    echo "expected answers artifact at $artifact_path" >&2
    exit 1
  }
  flow_doctor_assert_ok "$flow_path"
}

run_wizard_menu_smoke() {
  local flow_path
  local pack_dir
  local answers_path

  log "wizard menu smoke"
  flow_path="$(create_pack_with_main_flow "wizard-menu-smoke")"
  pack_dir="$(cd "$(dirname "$flow_path")/../../.." && pwd)"
  answers_path="$pack_dir/wizard.exit.answers.json"

  printf '0\n' | "$FLOW_BIN" wizard "$pack_dir" --emit-answers "$answers_path"
  [[ -f "$answers_path" ]] || {
    echo "expected wizard answers file at $answers_path" >&2
    exit 1
  }

  "$FLOW_BIN" wizard "$pack_dir" --answers-file "$answers_path" </dev/null
}

main() {
  run_wizard_menu_smoke

  while IFS=$'\t' read -r component_id component_ref; do
    run_add_step_case "$component_id" "$component_ref" "default" "interactive"
    run_add_step_case "$component_id" "$component_ref" "default" "answers-file"
    run_add_step_case "$component_id" "$component_ref" "personalised" "interactive"
    run_add_step_case "$component_id" "$component_ref" "personalised" "answers-file"
  done < <(component_rows)
}

main "$@"
