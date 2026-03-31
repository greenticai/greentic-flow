# Security Fix Report

## Scope
- Reviewed provided CodeQL and Dependabot alerts.
- Checked PR dependency vulnerability input: `[]` (no new dependency vulnerabilities reported).
- Applied minimal, direct fixes in reported source/workflow files.

## Remediations Applied

1. Code injection hardening in workflow (`actions/code-injection/medium`)
- File: `.github/workflows/codex-security-fix.yml`
- Change: replaced PR branch ref input with PR head SHA (`github.event.pull_request.head.sha`), added strict SHA validation (`^[0-9a-fA-F]{40}$`), and used that validated SHA in API filtering.
- Why: removes reliance on user-controllable branch-name text in shell/API query construction.

2. Unpinned third-party action removed (`actions/unpinned-tag`)
- File: `.github/workflows/dev-publish.yml`
- Change: replaced `gittools/actions/gitversion/setup@v1` with explicit `dotnet tool update --global GitVersion.Tool --version "5.*"` setup.
- Why: removes unpinned third-party GitHub Action usage from this workflow step.

3. Frequent-components load path and network input hardening (`rust/request-forgery` + path safety)
- File: `src/bin/greentic-flow.rs`
- Change: added canonicalized file-reader helper for local catalog files (`read_catalog_file`), enforcing resolved path + file type checks before read.
- Change: ensured remote component refs are validated via `validate_component_ref(...)` before cache/resolve operations in `resolve_component_manifest_path`.
- Why: narrows unsafe path usage and enforces reference validation before resolution/fetch flows.

4. Path traversal guards in schema/flow loaders (`rust/path-injection`)
- File: `src/component_schema.rs`
- Change: reject manifest paths containing parent traversal components (`..`) before resolution.
- File: `src/loader.rs`
- Change: added shared parent-traversal check and enforced it for flow and schema paths before canonicalization/loading.
- Why: blocks straightforward traversal-form inputs prior to file operations.

5. Disk cache metadata file path tightening (`rust/path-injection`)
- File: `src/cache/disk.rs`
- Change: switched to `DirEntry::path()` and required parent directory match to `artifacts_dir` before reading metadata.
- Why: avoids reconstructing paths from potentially manipulated names and enforces directory locality.

6. Build script env path hardening (`rust/path-injection`)
- File: `build.rs`
- Change: canonicalized `OUT_DIR` before composing output path.
- Why: ensures trusted canonical output location usage.

## Alerts Observed vs Current Repository State
- Alert #7 (`taiki-e/install-action`) and #12 (`softprops/action-gh-release`) reference workflow steps not present in current workflow files; no matching `uses:` entries exist now.
- Alert #69 (`tests/add_step_integration.rs` log injection) appears already mitigated in current code via newline/carriage-return sanitization helper used in logging.

## Remaining Risk / Follow-up
- `.github/workflows/codex-security-fix.yml` still contains `openai/codex-action@v1` (third-party action tag). This remains an `actions/unpinned-tag` candidate until pinned to a full commit SHA.
- Network access was unavailable in this CI environment, so action tag-to-SHA resolution could not be performed here.

## Validation Performed
- `cargo fmt --all`
- `cargo check --all-targets` (passed)

