use anyhow::Result;
use std::fs;
use std::path::Path;
use wasmtime::component::ResourceTable;
use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::p2;
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

use greentic_interfaces_wasmtime::repo_ui_actions_repo_ui_worker_v1_0::exports::greentic::repo_ui_actions::ui_action_api::ActionInput;
use greentic_interfaces_wasmtime::repo_ui_actions_repo_ui_worker_v1_0::RepoUiWorker;

const COMPONENT_PATH: &str = "tests/assets/repo_ui_actions_dummy.component.wasm";

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
fn repo_ui_actions_component_round_trip() -> Result<()> {
    let component_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(COMPONENT_PATH);
    if !component_path.exists() {
        eprintln!(
            "Skipping repo-ui-actions test; missing component at {}. \
             Run scripts/build-repo-ui-actions-dummy.sh to generate it.",
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

    let bindings = RepoUiWorker::instantiate(&mut store, &component, &linker)?;
    let ui = bindings.greentic_repo_ui_actions_ui_action_api();
    let tenant = "tenant-a".to_string();
    let page = "repositories".to_string();
    let action = "echo".to_string();
    let input = ActionInput {
        payload: "hello".to_string(),
    };
    let result = ui.call_handle_action(&mut store, &tenant, &page, &action, &input)?;

    assert!(result.success);
    assert_eq!(result.payload.as_deref(), Some("hello"));
    Ok(())
}
