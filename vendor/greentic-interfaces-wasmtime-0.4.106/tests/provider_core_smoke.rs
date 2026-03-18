use std::fs;
use std::path::Path;

use anyhow::Result;
use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::p2;
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

use greentic_interfaces_wasmtime::schema_core_v1_0::SchemaCore;

const COMPONENT_PATH: &str = "../../target/wasm32-wasip1/release/provider_core_dummy.wasm";

#[test]
fn provider_core_smoke() -> Result<()> {
    let component_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(COMPONENT_PATH);
    if !component_path.exists() {
        eprintln!(
            "Skipping provider-core smoke test; build the dummy component first with:\n  cargo component build -p provider-core-dummy --target wasm32-wasip2 --release"
        );
        return Ok(());
    }
    let component_bytes = fs::read(&component_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", component_path.display()));

    let mut config = Config::new();
    config.wasm_component_model(true);
    let engine = Engine::new(&config)?;
    let component = Component::from_binary(&engine, &component_bytes)?;

    struct HostState {
        table: wasmtime::component::ResourceTable,
        wasi: WasiCtx,
    }

    impl WasiView for HostState {
        fn ctx(&mut self) -> WasiCtxView<'_> {
            WasiCtxView {
                ctx: &mut self.wasi,
                table: &mut self.table,
            }
        }
    }

    let wasi = WasiCtxBuilder::new().inherit_stdio().build();
    let mut store = Store::new(
        &engine,
        HostState {
            table: wasmtime::component::ResourceTable::default(),
            wasi,
        },
    );
    let mut linker = Linker::new(&engine);
    p2::add_to_linker_sync(&mut linker)?;

    let bindings = SchemaCore::instantiate(&mut store, &component, &linker)?;
    let api = bindings.greentic_provider_schema_core_schema_core_api();

    let manifest = api.call_describe(&mut store)?;
    let manifest_str = String::from_utf8(manifest)?;
    assert!(manifest_str.contains("example.dummy"));
    assert!(manifest_str.contains("\"echo\""));

    let validation = api.call_validate_config(&mut store, b"{}")?;
    assert_eq!(String::from_utf8(validation)?, "{\"valid\":true}");

    let health = api.call_healthcheck(&mut store)?;
    assert_eq!(String::from_utf8(health)?, "{\"status\":\"ok\"}");

    let input = br#"{"echo":true}"#.to_vec();
    let echoed = api.call_invoke(&mut store, "echo", &input)?;
    assert_eq!(echoed, input.as_slice());

    Ok(())
}
