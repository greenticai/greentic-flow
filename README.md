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

Dev-channel builds (`greentic-flow-dev`, installed by `gtc install --channel dev`)
report `1.2.<run-id>`, the same version as their `v1.2.<run-id>` release tag. A dev
build that reports `1.2.0-dev.0` predates greenticai/.github#262 and should be
reinstalled from a newer dev release.

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
- The CLI does not install a tracing or OpenTelemetry subscriber, so `RUST_LOG` and `OTEL_*` variables have no effect on `greentic-flow` itself (library `tracing` events are not emitted by the binary).
- `GREENTIC_FLOW_ALLOW_PROXY=1` lets remote asset/catalog fetches go through a proxy taken from the environment. By default the proxy environment is ignored so resolved DNS answers can be checked against private ranges and pinned (SSRF/DNS-rebinding protection). Fetches are https-only, so only `HTTPS_PROXY`/`https_proxy` and then `ALL_PROXY`/`all_proxy` are consulted (`HTTP_PROXY` is never used); within `HTTPS_PROXY`/`https_proxy` (and likewise `ALL_PROXY`/`all_proxy`) the first variable that is *set* wins, and an empty value means "no proxy" for that pair; an empty or unusable HTTPS value falls back to `ALL_PROXY`. The value must be an `http` or `https` URL (SOCKS is not supported and is ignored), a set `REQUEST_METHOD` (CGI) disables proxies, and `NO_PROXY`/`no_proxy` exclusions (`*`, exact host, domain suffix, optional port, and for IP-literal targets an exact IPv4/IPv6 address, bracketed IPv6, or CIDR such as `10.0.0.0/8` or `2001:db8::/32`) are honoured. The opt-in only takes effect when a proxy really applies to the request; otherwise the full default checks still run. When the proxy is used, static URL validation still applies but the private-address DNS check and DNS pinning are both skipped (left to the proxy) and a warning is printed to stderr.

## Maintenance Notes
- Keep shared primitives flowing through `greentic-types` and `greentic-interfaces`.
- Prefer zero-copy patterns and stay within safe Rust (the library uses `#![deny(unsafe_code)]` with two audited `#[allow(unsafe_code)]` sites: component-cache native deserialization in `src/cache/mod.rs` and `src/wizard_ops.rs`; the CLI binary additionally uses `unsafe` to set `GREENTIC_LOCALE` at startup).
- Update the adapter registry fixtures under `tests/data/` when new adapters or operations are introduced.
- Dependabot auto-merge is enabled for Cargo updates; repository settings must allow auto-merge and branch protections should list the required checks to gate merges.

## Releases & Publishing
- Crate versions are sourced directly from each crate's `Cargo.toml`.
- Every push to `main` runs `.github/workflows/tag-on-version-bump.yml` (a reusable workflow from `greenticai/.github`); when the crate version changed it creates and pushes a `v<semver>` tag.
- `v*` tags trigger `.github/workflows/release-binaries.yml`, which builds the release binaries and then runs its `publish_crates` job to publish the crate to crates.io (via the shared `crates-publish.yml`). `flowcrates-v*` tags (or a manual dispatch) trigger `.github/workflows/crates-publish.yml`, which publishes the crate to crates.io.
- Publishing is idempotent: if the version already exists on crates.io, the workflow succeeds without error.
