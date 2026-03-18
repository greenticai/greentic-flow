# Greentic Interfaces Wasmtime Runtime Helpers

`greentic-interfaces-wasmtime` provides the Wasmtime integration layer for the Greentic platform. It wires host services into a Wasmtime component linker, offers convenience helpers for building engines, and exposes mapper utilities that bridge the ABI structs published by [`greentic-interfaces`](../greentic-interfaces) with the richer models from [`greentic-types`](https://github.com/greentic-ai/greentic-types).

## Feature flags

- `stable-wasmtime` (default): builds against Wasmtime releases `< 38`, compatible with the stable Rust channel.
- `nightly-wasmtime`: switches to Wasmtime `38.0.3`, which currently requires the nightly toolchain due to edition 2024 support and fiber features.

Enable the right flag depending on your toolchain:

```toml
[dependencies]
greentic-interfaces-wasmtime = { version = "0.1", default-features = false, features = ["nightly-wasmtime"] }
```

## Quick start

```rust
use greentic_interfaces_wasmtime::{build_engine, EngineOptions, LinkerBuilder};

let engine = build_engine(EngineOptions::default())?;
let mut linker = LinkerBuilder::new(&engine).finish();
// Register host services as needed:
// greentic_interfaces_wasmtime::add_secrets_store_to_linker(&mut linker, |state| &mut state.secrets)?;
```

### Host helpers

Use the stable `host_helpers::v1::*` façade to wire host-import worlds without reaching into generated module paths:

```rust
use greentic_interfaces_wasmtime::host_helpers::v1::{self, HostFns};
use wasmtime::component::Linker;

let mut linker: Linker<MyHostState> = Linker::new(&engine);
v1::add_all_v1_to_linker(
    &mut linker,
    HostFns {
        http_client_v1_1: Some(|state| &mut state.http),
        http_client: None, // legacy v1.0.0 only; ignored when v1_1 is set
        oauth_broker: Some(|state| &mut state.oauth),
        runner_host_http: Some(|state| &mut state.runner_http),
        runner_host_kv: Some(|state| &mut state.runner_kv),
        telemetry_logger: Some(|state| &mut state.telemetry),
        state_store: Some(|state| &mut state.state),
        secrets_store: Some(|state| &mut state.secrets),
    },
)?;
```

Implement the `Host` trait from each `v1` module (e.g. `v1::http_client::HttpClientHostV1_1` for HTTP or `HttpClientHost` for the legacy `@1.0.0`) on your host state and keep the generated module paths internal.
