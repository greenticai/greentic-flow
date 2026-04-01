#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${1:-all}"
AUTH_MODE="${AUTH_MODE:-auto}"
LOCALE="${LOCALE:-en}"
TRANSLATOR_BIN="${TRANSLATOR_BIN:-greentic-i18n-translator}"
BINSTALL_BIN="${BINSTALL_BIN:-cargo-binstall}"
BATCH_SIZE="${BATCH_SIZE:-500}"
LOCAL_CACHE_DIR="${LOCAL_CACHE_DIR:-$ROOT_DIR/.i18n/cache}"
default_global_cache_dir() {
  case "$(uname -s)" in
    Darwin)
      printf '%s\n' "$HOME/Library/Caches/greentic/i18n-translator"
      ;;
    Linux)
      printf '%s\n' "${XDG_CACHE_HOME:-$HOME/.cache}/greentic/i18n-translator"
      ;;
    *)
      printf '%s\n' "${XDG_CACHE_HOME:-$HOME/.cache}/greentic/i18n-translator"
      ;;
  esac
}

GLOBAL_CACHE_DIR="${GLOBAL_CACHE_DIR:-$(default_global_cache_dir)}"
CACHE_DIR="${CACHE_DIR:-$GLOBAL_CACHE_DIR}"

scrub_cache_dir() {
  local dir="$1"
  if [[ ! -d "$dir" ]]; then
    return 0
  fi

  if command -v python3 >/dev/null 2>&1; then
    python3 - "$dir" <<'PY'
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
for path in root.rglob("*.json"):
    try:
        with path.open("r", encoding="utf-8") as handle:
            json.load(handle)
    except Exception as exc:
        print(f"removing invalid cache entry {path}: {exc}", file=sys.stderr)
        try:
            path.unlink()
        except FileNotFoundError:
            pass
PY
    return 0
  fi

  find "$dir" -type f -name '*.json' -size 0 -print0 | while IFS= read -r -d '' file; do
    echo "removing empty cache entry $file" >&2
    rm -f "$file"
  done
}

ensure_translator() {
  if command -v "$TRANSLATOR_BIN" >/dev/null 2>&1; then
    return 0
  fi

  if ! command -v "$BINSTALL_BIN" >/dev/null 2>&1; then
    echo "missing translator binary: $TRANSLATOR_BIN" >&2
    echo "missing installer binary: $BINSTALL_BIN" >&2
    echo "install cargo-binstall or set TRANSLATOR_BIN=/path/to/greentic-i18n-translator" >&2
    exit 2
  fi

  echo "installing $TRANSLATOR_BIN via $BINSTALL_BIN" >&2
  "$BINSTALL_BIN" -y greentic-i18n-translator

  if ! command -v "$TRANSLATOR_BIN" >/dev/null 2>&1; then
    echo "failed to install translator binary: $TRANSLATOR_BIN" >&2
    exit 2
  fi
}

merge_local_cache_into_global() {
  if [[ "$CACHE_DIR" != "$GLOBAL_CACHE_DIR" ]]; then
    return 0
  fi

  if [[ ! -d "$LOCAL_CACHE_DIR" ]]; then
    return 0
  fi

  mkdir -p "$GLOBAL_CACHE_DIR"

  find "$LOCAL_CACHE_DIR" -type f -name '*.json' -print0 | while IFS= read -r -d '' file; do
    local rel
    rel="${file#$LOCAL_CACHE_DIR/}"
    local dest="$GLOBAL_CACHE_DIR/$rel"
    mkdir -p "$(dirname "$dest")"
    if [[ ! -f "$dest" ]]; then
      cp "$file" "$dest"
    fi
  done
}

DEFAULT_EN_PATHS=(
  "$ROOT_DIR/i18n/en.json"
  "$ROOT_DIR/i18n/wizard/en.json"
)

resolve_en_paths() {
  if [[ -n "${EN_PATH:-}" ]]; then
    printf '%s\n' "$EN_PATH"
    return 0
  fi
  printf '%s\n' "${DEFAULT_EN_PATHS[@]}"
}

base_langs_csv() {
  local base_dir="$ROOT_DIR/i18n"
  ls -1 "$base_dir"/*.json 2>/dev/null \
    | xargs -n1 basename \
    | sed 's/\.json$//' \
    | grep -v '^en$' \
    | sort -u \
    | paste -sd, -
}

langs_for_en_path() {
  local en_path="$1"
  if [[ -n "${LANGS:-}" ]]; then
    printf '%s\n' "$LANGS"
    return 0
  fi

  if [[ "$en_path" == "$ROOT_DIR/i18n/wizard/en.json" ]]; then
    local langs
    langs="$(base_langs_csv)"
    if [[ -n "$langs" ]]; then
      printf '%s\n' "$langs"
      return 0
    fi
  fi

  printf '%s\n' "all"
}

run_for_path() {
  local mode="$1"
  local en_path="$2"
  local langs="$3"
  local cmd=("$TRANSLATOR_BIN" --locale "$LOCALE" "$mode" --langs "$langs" --en "$en_path")
  if [[ "$mode" == "translate" ]]; then
    cmd+=(--auth-mode "$AUTH_MODE" --cache-dir "$CACHE_DIR" --batch-size "$BATCH_SIZE")
  fi
  (cd "$ROOT_DIR" && "${cmd[@]}")
}

ensure_translator
scrub_cache_dir "$LOCAL_CACHE_DIR"
scrub_cache_dir "$GLOBAL_CACHE_DIR"
merge_local_cache_into_global
if [[ "$CACHE_DIR" != "$GLOBAL_CACHE_DIR" && "$CACHE_DIR" != "$LOCAL_CACHE_DIR" ]]; then
  scrub_cache_dir "$CACHE_DIR"
fi

while IFS= read -r path; do
  if [[ ! -f "$path" ]]; then
    echo "missing English source map: $path" >&2
    exit 2
  fi
  abs_path="$(cd "$(dirname "$path")" && pwd)/$(basename "$path")"
  langs="$(langs_for_en_path "$abs_path")"
  case "$MODE" in
    translate)
      echo "==> translate: $abs_path"
      run_for_path "translate" "$abs_path" "$langs"
      ;;
    validate)
      echo "==> validate: $abs_path"
      run_for_path "validate" "$abs_path" "$langs"
      ;;
    status)
      echo "==> status: $abs_path"
      run_for_path "status" "$abs_path" "$langs"
      ;;
    all)
      echo "==> translate: $abs_path"
      run_for_path "translate" "$abs_path" "$langs"
      echo "==> validate: $abs_path"
      run_for_path "validate" "$abs_path" "$langs"
      echo "==> status: $abs_path"
      run_for_path "status" "$abs_path" "$langs"
      ;;
    *)
      echo "Unknown mode: $MODE" >&2
      exit 2
      ;;
  esac
done < <(resolve_en_paths)
