# Greentic Flow

Human-friendly YGTc v2 flow authoring: create flows, add component steps, keep routing safe, and validate everything with one CLI.

Canonical docs in this repository track the v0.6 model. For historical compatibility notes, use `docs/vision/legacy.md`.

Coding agents and automation should not treat this README as the full authoring contract. Use [`docs/wizard/authoring.md`](docs/wizard/authoring.md) and prefer `greentic-flow wizard --schema`, `greentic-flow component-schema`, and `greentic-flow wizard <pack> --answers <plan.json>`.

## Why flows?
- **Readable YAML**: node key = node name, one operation key inside, routing shorthand (`out|reply|[...]`).
- **Component-free authoring**: flows stay human; component sources live in a sidecar resolve file.
- **Safe edits**: add/update/delete steps rewrite routing deterministically and validate against the schema.
- **CI-ready**: built-in validator (`doctor`) and binstall-friendly releases.

## Install
- GitHub Releases (binstall-ready): `cargo binstall greentic-flow`
- crates.io (no bundled binaries): `cargo install --locked greentic-flow`
- Direct download: pick the `.tgz` for your target from the latest release and put `greentic-flow` on your `PATH`.

Check the installed CLI version:

```bash
greentic-flow --version
```

## Create your first flow

```bash
greentic-flow new --flow ./hello.ygtc --id hello-flow --type messaging \
  --name "Hello Flow"
```

`new` writes an empty v2 skeleton (`nodes: {}`) so you can start from a clean slate. If you want a ready-to-run “hello” flow, copy the example file we keep in the repo:

```bash
cp docs/examples/hello.ygtc /tmp/hello.ygtc
```

That example (also covered by tests) is small and readable:

```yaml
id: hello-flow
type: messaging
schema_version: 2
start: start
nodes:
  start:
    templating.handlebars:
      text: "Hello from greentic-flow!"
    routing: out
```

## Preferred Authoring Path

For day-to-day authoring, prefer the pack-level wizard instead of editing `*.ygtc` by hand.

Interactive use:

```bash
greentic-flow wizard /path/to/pack
```

Replay a saved plan without prompts:

```bash
greentic-flow wizard /path/to/pack --answers plan.json
```

The wizard works best when your project is organized as a pack root:

```text
/path/to/pack/
  pack.yaml
  flows/
    main.ygtc
```

If you are building components locally, place their wasm files under `components/` in the pack or use pinned public component references. The wizard keeps the flow, sidecar, and pack metadata aligned.

If you want the full schema-driven workflow for automation, coding agents, or CI replay, use the dedicated guide: [`docs/wizard/authoring.md`](docs/wizard/authoring.md).

## Wizard and Capability Boundaries
- `greentic-flow` orchestrates canonical setup via `describe()` + `invoke("setup.apply_answers")` and flow/sidecar updates.
- Capability gating is enforced by the runtime/operator host, not by `greentic-flow`.
- Wizard summaries can display requested/provided capability groups from component `describe` output for operator visibility.
- Wizard mode is `default|setup|update|remove`.

## Validate flows (CI & local)

```
greentic-flow doctor flows/                   # recursive over .ygtc
greentic-flow doctor --json flows/main.ygtc   # machine-readable
```

Uses the embedded `schemas/ygtc.flow.schema.json` by default; add `--registry <adapter_catalog.json>` for adapter linting.

## Deep dives
- Docs index: [`docs/README.md`](docs/README.md)
- Preferred wizard authoring guide: [`docs/wizard/authoring.md`](docs/wizard/authoring.md)
- CLI details and routing flags: [`docs/cli.md`](docs/cli.md)
- Add-step design and routing rules: [`docs/add_step_design.md`](docs/add_step_design.md)
- Deployment flows: [`docs/deployment-flows.md`](docs/deployment-flows.md)
- Config flow execution: [`docs/add_step_design.md`](docs/add_step_design.md#config-mode)
- Vision and legacy compatibility: [`docs/vision/README.md`](docs/vision/README.md)

## Development
- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`

Or run everything: `LOCAL_CHECK_ONLINE=1 ci/local_check.sh`

## Environment
- `OTEL_EXPORTER_OTLP_ENDPOINT` (default `http://localhost:4317`) targets your collector.
- `RUST_LOG` controls log verbosity; e.g. `greentic_flow=info`.
- `OTEL_RESOURCE_ATTRIBUTES=deployment.environment=dev` tags spans with the active environment.

## Maintenance Notes
- Keep shared primitives flowing through `greentic-types` and `greentic-interfaces`.
- Prefer zero-copy patterns and stay within safe Rust (`#![forbid(unsafe_code)]` is enabled).
- Update the adapter registry fixtures under `tests/data/` when new adapters or operations are introduced.
- Dependabot auto-merge is enabled for Cargo updates; repository settings must allow auto-merge and branch protections should list the required checks to gate merges.

## Releases & Publishing
- Crate versions are sourced directly from each crate's `Cargo.toml`.
- Every push to `master` compares the previous commit; if a crate version changed, a tag `<crate-name>-v<semver>` is created and pushed automatically.
- The publish workflow runs on the tagged commit and attempts to publish all changed crates to crates.io using `katyo/publish-crates@v2`.
- Publishing is idempotent: if the version already exists on crates.io, the workflow succeeds without error.
