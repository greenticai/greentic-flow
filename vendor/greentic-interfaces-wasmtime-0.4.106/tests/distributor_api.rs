use anyhow::Result;
use std::fs;
use std::path::Path;
use wasmtime::component::ResourceTable;
use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::p2;
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

use greentic_interfaces_wasmtime::distributor_distributor_api_v1_0::exports::greentic::distributor_api::distributor::{
    ComponentStatus, ResolveComponentRequest,
};
use greentic_interfaces_wasmtime::distributor_distributor_api_v1_0::DistributorApi;

const COMPONENT_PATH: &str = "tests/assets/distributor_api_dummy.component.wasm";

#[derive(Default)]
struct HostState {
    table: ResourceTable,
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

#[test]
fn distributor_api_component_round_trip() -> Result<()> {
    let component_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(COMPONENT_PATH);
    if !component_path.exists() {
        eprintln!(
            "Skipping distributor-api test; missing component at {}. \
             Run scripts/build-distributor-api-dummy.sh to generate it.",
            component_path.display()
        );
        return Ok(());
    }

    let component_bytes = fs::read(&component_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", component_path.display()));

    let mut config = Config::new();
    config.wasm_component_model(true);
    let engine = Engine::new(&config)?;
    let component = Component::from_binary(&engine, &component_bytes)?;
    let mut store = Store::new(
        &engine,
        HostState {
            table: Default::default(),
            wasi: WasiCtxBuilder::new().build(),
        },
    );
    let mut linker = Linker::new(&engine);
    p2::add_to_linker_sync(&mut linker)?;

    let bindings = DistributorApi::instantiate(&mut store, &component, &linker)?;
    let api = bindings.greentic_distributor_api_distributor();

    let req = ResolveComponentRequest {
        tenant_id: "tenant-a".to_string(),
        environment_id: "env-a".to_string(),
        pack_id: "pack-a".to_string(),
        component_id: "component-a".to_string(),
        version: "1.0.0".to_string(),
        extra: "{}".to_string(),
    };

    let resp = api.call_resolve_component(&mut store, &req)?;
    assert!(matches!(resp.component_status, ComponentStatus::Ready));
    assert!(resp.digest.starts_with("sha256:"));
    assert_eq!(resp.artifact_location.kind, "file");
    assert_eq!(resp.artifact_location.value, "/tmp/dummy.component.wasm");
    assert!(!resp.signature_summary.verified);
    assert_eq!(resp.signature_summary.signer, "n/a");
    assert_eq!(resp.signature_summary.extra, "{}");
    assert_eq!(resp.cache_info.size_bytes, 0);
    assert_eq!(resp.cache_info.last_used_utc, "1970-01-01T00:00:00Z");
    assert_eq!(resp.cache_info.last_refreshed_utc, "1970-01-01T00:00:00Z");
    assert!(resp.secret_requirements.is_empty());

    let tenant = "tenant-a".to_string();
    let env = "env-a".to_string();
    let pack = "pack-a".to_string();
    let status = api.call_get_pack_status(&mut store, &tenant, &env, &pack)?;
    assert_eq!(status, "\"ok\"");
    let status_v2 = api.call_get_pack_status_v2(&mut store, &tenant, &env, &pack)?;
    assert_eq!(status_v2.status, "ok");
    assert!(status_v2.secret_requirements.is_empty());
    assert_eq!(status_v2.extra, "{}");

    api.call_warm_pack(&mut store, &tenant, &env, &pack)?;

    Ok(())
}
