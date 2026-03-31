use std::collections::{BTreeMap, HashMap};

use anyhow::{Result, anyhow};
use serde_json::Value as JsonValue;

use crate::i18n::{I18nCatalog, resolve_text};
use greentic_interfaces_host::component_v0_6::exports::greentic::component::node::{
    ComponentDescriptor, SchemaSource,
};
use greentic_types::cbor::canonical;
use greentic_types::schemas::component::v0_6_0::{ComponentQaSpec, QaMode, QuestionKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WizardAbi {
    V6,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WizardMode {
    Default,
    Setup,
    Update,
    Remove,
}

impl WizardMode {
    pub fn as_str(self) -> &'static str {
        match self {
            WizardMode::Default => "default",
            WizardMode::Setup => "setup",
            WizardMode::Update => "update",
            WizardMode::Remove => "remove",
        }
    }

    pub fn as_qa_mode(self) -> QaMode {
        match self {
            WizardMode::Default => QaMode::Default,
            WizardMode::Setup => QaMode::Setup,
            WizardMode::Update => QaMode::Update,
            WizardMode::Remove => QaMode::Remove,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WizardOutput {
    pub abi: WizardAbi,
    pub describe_cbor: Vec<u8>,
    pub descriptor: Option<ComponentDescriptor>,
    pub qa_spec_cbor: Vec<u8>,
    pub answers_cbor: Vec<u8>,
    pub config_cbor: Vec<u8>,
}

#[cfg(not(target_arch = "wasm32"))]
pub struct WizardSpecOutput {
    pub abi: WizardAbi,
    pub describe_cbor: Vec<u8>,
    pub descriptor: Option<ComponentDescriptor>,
    pub qa_spec_cbor: Vec<u8>,
    pub answers_schema_cbor: Option<Vec<u8>>,
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(unsafe_code)]
mod host {
    use super::*;
    use std::sync::{Arc, OnceLock};

    use crate::cache::{ArtifactKey, CacheConfig, CacheManager, CpuPolicy, EngineProfile};
    use greentic_interfaces_host::component_v0_6::exports::greentic::component::node as canonical_node;
    use greentic_interfaces_wasmtime::host_helpers::v1::{
        self, HostFns,
        http_client::{
            HttpClientErrorV1_1, HttpClientHostV1_1, RequestOptionsV1_1, RequestV1_1, ResponseV1_1,
            TenantCtxV1_1,
        },
        oauth_broker::OAuthBrokerHost,
        runner_host_http::RunnerHostHttp,
        runner_host_kv::RunnerHostKv,
        secrets_store::{SecretsErrorV1_1, SecretsStoreHostV1_1},
        state_store::{
            OpAck, StateKey, StateStoreError, StateStoreHost, TenantCtx as StateTenantCtx,
        },
        telemetry_logger::{
            OpAck as TelemetryOpAck, SpanContext, TelemetryLoggerError, TelemetryLoggerHost,
            TenantCtx,
        },
    };
    use wasmtime::component::{Component, Linker};
    use wasmtime::component::{ResourceTable, Val};
    use wasmtime::{Config, Engine, Store, StoreContextMut};
    use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

    mod runtime {
        pub use greentic_interfaces_host::component_v0_6::exports::greentic::component::node;
        pub use greentic_interfaces_host::component_v0_6::greentic::types_core::core;
        pub type RuntimeComponent = greentic_interfaces_host::component_v0_6::ComponentV0V6V0;
    }

    struct HostState {
        wasi: WasiCtx,
        table: ResourceTable,
        http_client: OfflineHttpClient,
        oauth_broker: OfflineOAuthBroker,
        runner_http: OfflineRunnerHostHttp,
        runner_kv: OfflineRunnerHostKv,
        telemetry_logger: NoopTelemetryLogger,
        state_store: NoopStateStore,
        secrets_store: NoopSecretsStore,
    }

    struct NoopStateStore;
    struct NoopTelemetryLogger;
    struct NoopSecretsStore;
    struct OfflineHttpClient;
    struct OfflineOAuthBroker;
    struct OfflineRunnerHostHttp;
    struct OfflineRunnerHostKv;

    struct WizardRuntimeCache {
        engine: Engine,
        component_cache: CacheManager,
        async_runtime: tokio::runtime::Runtime,
    }

    impl StateStoreHost for NoopStateStore {
        fn read(
            &mut self,
            _key: StateKey,
            _ctx: Option<StateTenantCtx>,
        ) -> std::result::Result<Vec<u8>, StateStoreError> {
            Ok(Vec::new())
        }

        fn write(
            &mut self,
            _key: StateKey,
            _bytes: Vec<u8>,
            _ctx: Option<StateTenantCtx>,
        ) -> std::result::Result<OpAck, StateStoreError> {
            Ok(OpAck::Ok)
        }

        fn delete(
            &mut self,
            _key: StateKey,
            _ctx: Option<StateTenantCtx>,
        ) -> std::result::Result<OpAck, StateStoreError> {
            Ok(OpAck::Ok)
        }
    }

    impl TelemetryLoggerHost for NoopTelemetryLogger {
        fn log(
            &mut self,
            _span: SpanContext,
            _fields: wasmtime::component::__internal::Vec<(
                wasmtime::component::__internal::String,
                wasmtime::component::__internal::String,
            )>,
            _ctx: Option<TenantCtx>,
        ) -> std::result::Result<TelemetryOpAck, TelemetryLoggerError> {
            Ok(TelemetryOpAck::Ok)
        }
    }

    impl SecretsStoreHostV1_1 for NoopSecretsStore {
        fn get(
            &mut self,
            _key: wasmtime::component::__internal::String,
        ) -> std::result::Result<Option<wasmtime::component::__internal::Vec<u8>>, SecretsErrorV1_1>
        {
            Ok(None)
        }

        fn put(
            &mut self,
            _key: wasmtime::component::__internal::String,
            _value: wasmtime::component::__internal::Vec<u8>,
        ) {
        }
    }

    impl HttpClientHostV1_1 for OfflineHttpClient {
        fn send(
            &mut self,
            _req: RequestV1_1,
            _opts: Option<RequestOptionsV1_1>,
            _ctx: Option<TenantCtxV1_1>,
        ) -> std::result::Result<ResponseV1_1, HttpClientErrorV1_1> {
            Ok(ResponseV1_1 {
                status: 204,
                headers: Vec::new(),
                body: None,
            })
        }
    }

    impl OAuthBrokerHost for OfflineOAuthBroker {
        fn get_consent_url(
            &mut self,
            _provider_id: wasmtime::component::__internal::String,
            _subject: wasmtime::component::__internal::String,
            _scopes: wasmtime::component::__internal::Vec<wasmtime::component::__internal::String>,
            _redirect_path: wasmtime::component::__internal::String,
            _extra_json: wasmtime::component::__internal::String,
        ) -> wasmtime::component::__internal::String {
            "offline://oauth-disabled".into()
        }

        fn exchange_code(
            &mut self,
            _provider_id: wasmtime::component::__internal::String,
            _subject: wasmtime::component::__internal::String,
            _code: wasmtime::component::__internal::String,
            _redirect_path: wasmtime::component::__internal::String,
        ) -> wasmtime::component::__internal::String {
            String::new()
        }

        fn get_token(
            &mut self,
            _provider_id: wasmtime::component::__internal::String,
            _subject: wasmtime::component::__internal::String,
            _scopes: wasmtime::component::__internal::Vec<wasmtime::component::__internal::String>,
        ) -> wasmtime::component::__internal::String {
            String::new()
        }
    }

    impl RunnerHostHttp for OfflineRunnerHostHttp {
        fn request(
            &mut self,
            _method: wasmtime::component::__internal::String,
            _url: wasmtime::component::__internal::String,
            _headers: wasmtime::component::__internal::Vec<wasmtime::component::__internal::String>,
            _body: Option<wasmtime::component::__internal::Vec<u8>>,
        ) -> std::result::Result<
            wasmtime::component::__internal::Vec<u8>,
            wasmtime::component::__internal::String,
        > {
            Ok(Vec::new())
        }
    }

    impl RunnerHostKv for OfflineRunnerHostKv {
        fn get(
            &mut self,
            _ns: wasmtime::component::__internal::String,
            _key: wasmtime::component::__internal::String,
        ) -> Option<wasmtime::component::__internal::String> {
            None
        }

        fn put(
            &mut self,
            _ns: wasmtime::component::__internal::String,
            _key: wasmtime::component::__internal::String,
            _val: wasmtime::component::__internal::String,
        ) {
        }
    }

    impl HostState {
        fn new() -> Self {
            Self {
                // Keep a minimal WASI context; this still provides the imports
                // expected by components that read CLI env/args.
                wasi: WasiCtxBuilder::new().build(),
                table: ResourceTable::new(),
                http_client: OfflineHttpClient,
                oauth_broker: OfflineOAuthBroker,
                runner_http: OfflineRunnerHostHttp,
                runner_kv: OfflineRunnerHostKv,
                telemetry_logger: NoopTelemetryLogger,
                state_store: NoopStateStore,
                secrets_store: NoopSecretsStore,
            }
        }
    }

    impl WasiView for HostState {
        fn ctx(&mut self) -> WasiCtxView<'_> {
            WasiCtxView {
                ctx: &mut self.wasi,
                table: &mut self.table,
            }
        }
    }

    fn build_engine() -> Result<Engine> {
        let mut config = Config::new();
        config.wasm_component_model(true);
        Engine::new(&config).map_err(|err| anyhow!("init wasm engine: {err}"))
    }

    fn wizard_runtime_cache() -> Result<&'static WizardRuntimeCache> {
        static RUNTIME: OnceLock<Result<WizardRuntimeCache, String>> = OnceLock::new();
        let runtime =
            RUNTIME.get_or_init(|| WizardRuntimeCache::new().map_err(|err| format!("{err:#}")));
        runtime
            .as_ref()
            .map_err(|message| anyhow!("init wizard wasm runtime cache: {message}"))
    }

    impl WizardRuntimeCache {
        fn new() -> Result<Self> {
            let engine = build_engine()?;
            let profile =
                EngineProfile::from_engine(&engine, CpuPolicy::Native, "default".to_string());
            let component_cache = CacheManager::new(CacheConfig::default(), profile);
            let async_runtime = tokio::runtime::Runtime::new()
                .map_err(|err| anyhow!("init wizard cache async runtime: {err}"))?;
            Ok(Self {
                engine,
                component_cache,
                async_runtime,
            })
        }
    }

    fn wizard_engine() -> Result<&'static Engine> {
        Ok(&wizard_runtime_cache()?.engine)
    }

    fn compute_sha256_digest_for(bytes: &[u8]) -> String {
        use sha2::Digest as _;

        let mut hasher = sha2::Sha256::new();
        hasher.update(bytes);
        format!("sha256:{:x}", hasher.finalize())
    }

    fn load_component_cached(wasm_bytes: &[u8]) -> Result<Arc<Component>> {
        let runtime = wizard_runtime_cache()?;
        let key = ArtifactKey::new(
            runtime.component_cache.engine_profile_id().to_string(),
            compute_sha256_digest_for(wasm_bytes),
        );
        let fut = runtime
            .component_cache
            .get_component(&runtime.engine, &key, || Ok(wasm_bytes.to_vec()));
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            tokio::task::block_in_place(|| handle.block_on(fut))
        } else {
            runtime.async_runtime.block_on(fut)
        }
    }

    fn add_wasi_imports(linker: &mut Linker<HostState>) -> Result<()> {
        wasmtime_wasi::p2::add_to_linker_sync(linker)
            .map_err(|err| anyhow!("link wasi imports: {err}"))?;
        v1::add_all_v1_to_linker(
            linker,
            HostFns {
                http_client_v1_1: Some(|state: &mut HostState| &mut state.http_client),
                http_client: None,
                oauth_broker: Some(|state: &mut HostState| &mut state.oauth_broker),
                runner_host_http: Some(|state: &mut HostState| &mut state.runner_http),
                runner_host_kv: Some(|state: &mut HostState| &mut state.runner_kv),
                telemetry_logger: Some(|state: &mut HostState| &mut state.telemetry_logger),
                state_store: Some(|state: &mut HostState| &mut state.state_store),
                secrets_store_v1_1: Some(|state: &mut HostState| &mut state.secrets_store),
                secrets_store: None,
            },
        )
        .map_err(|err| anyhow!("link Greentic v1 host imports: {err}"))?;
        add_wasi_cli_environment_0_2_3_compat(linker)?;
        Ok(())
    }

    fn add_wasi_cli_environment_0_2_3_compat(linker: &mut Linker<HostState>) -> Result<()> {
        let mut inst = linker
            .instance("wasi:cli/environment@0.2.3")
            .map_err(|err| anyhow!("link wasi:cli/environment@0.2.3 import: {err}"))?;
        inst.func_wrap(
            "get-environment",
            |_caller: StoreContextMut<'_, HostState>,
             (): ()|
             -> wasmtime::Result<(Vec<(String, String)>,)> { Ok((Vec::new(),)) },
        )
        .map_err(|err| anyhow!("link wasi:cli/environment@0.2.3.get-environment: {err}"))?;
        inst.func_wrap(
            "get-arguments",
            |_caller: StoreContextMut<'_, HostState>, (): ()| -> wasmtime::Result<(Vec<String>,)> {
                Ok((Vec::new(),))
            },
        )
        .map_err(|err| anyhow!("link wasi:cli/environment@0.2.3.get-arguments: {err}"))?;
        inst.func_wrap(
            "initial-cwd",
            |_caller: StoreContextMut<'_, HostState>,
             (): ()|
             -> wasmtime::Result<(Option<String>,)> { Ok((None,)) },
        )
        .map_err(|err| anyhow!("link wasi:cli/environment@0.2.3.initial-cwd: {err}"))?;
        Ok(())
    }

    fn add_control_imports(linker: &mut Linker<HostState>) -> Result<()> {
        let mut inst = linker
            .instance("greentic:component/control@0.6.0")
            .map_err(|err| anyhow!("link control import: {err}"))?;
        inst.func_wrap(
            "should-cancel",
            |_caller: StoreContextMut<'_, HostState>, (): ()| -> wasmtime::Result<(bool,)> {
                Ok((false,))
            },
        )
        .map_err(|err| anyhow!("link control.should-cancel: {err}"))?;
        inst.func_wrap(
            "yield-now",
            |_caller: StoreContextMut<'_, HostState>, (): ()| -> wasmtime::Result<()> { Ok(()) },
        )
        .map_err(|err| anyhow!("link control.yield-now: {err}"))?;
        Ok(())
    }

    fn schema_source_to_cbor(source: &SchemaSource, label: &str) -> Result<Vec<u8>> {
        match source {
            SchemaSource::InlineCbor(bytes) => Ok(bytes.clone()),
            SchemaSource::CborSchemaId(id) => Err(anyhow!(
                "{label} uses cbor-schema-id '{id}', but greentic-flow requires inline-cbor for wizard execution"
            )),
            SchemaSource::RefPackPath(path) => Err(anyhow!(
                "{label} uses ref-pack-path '{path}', but greentic-flow requires inline-cbor for wizard execution"
            )),
            SchemaSource::RefUri(uri) => Err(anyhow!(
                "{label} uses ref-uri '{uri}', but greentic-flow requires inline-cbor for wizard execution"
            )),
        }
    }

    pub(super) fn extract_setup_contract(
        descriptor: &ComponentDescriptor,
    ) -> Result<(Vec<u8>, Option<Vec<u8>>)> {
        let qa_ref = crate::component_setup::qa_spec_ref(descriptor)
            .ok_or_else(|| anyhow!("component descriptor missing setup.qa-spec"))?;
        let qa_spec_cbor = schema_source_to_cbor(qa_ref, "setup.qa-spec")?;
        let answers_schema_cbor = crate::component_setup::answers_schema_ref(descriptor)
            .map(|source| schema_source_to_cbor(source, "setup.answers-schema"))
            .transpose()?;
        Ok((qa_spec_cbor, answers_schema_cbor))
    }

    pub(super) fn ensure_setup_apply_answers_op(descriptor: &ComponentDescriptor) -> Result<()> {
        if descriptor
            .ops
            .iter()
            .any(|op| op.name == "setup.apply_answers")
        {
            return Ok(());
        }
        Err(anyhow!(
            "component descriptor does not advertise required op 'setup.apply_answers'"
        ))
    }

    fn invoke_envelope(payload_cbor: Vec<u8>) -> runtime::node::InvocationEnvelope {
        runtime::node::InvocationEnvelope {
            ctx: runtime::core::TenantCtx {
                tenant_id: "local".to_string(),
                team_id: None,
                user_id: None,
                env_id: "local".to_string(),
                trace_id: "trace-local".to_string(),
                correlation_id: "corr-local".to_string(),
                deadline_ms: 0,
                attempt: 0,
                idempotency_key: None,
                i18n_id: "en-US".to_string(),
            },
            flow_id: "wizard-flow".to_string(),
            step_id: "wizard-step".to_string(),
            component_id: "component".to_string(),
            attempt: 0,
            payload_cbor,
            metadata_cbor: None,
        }
    }

    fn convert_schema_source(source: runtime::node::SchemaSource) -> canonical_node::SchemaSource {
        match source {
            runtime::node::SchemaSource::CborSchemaId(id) => {
                canonical_node::SchemaSource::CborSchemaId(id)
            }
            runtime::node::SchemaSource::InlineCbor(bytes) => {
                canonical_node::SchemaSource::InlineCbor(bytes)
            }
            runtime::node::SchemaSource::RefPackPath(path) => {
                canonical_node::SchemaSource::RefPackPath(path)
            }
            runtime::node::SchemaSource::RefUri(uri) => canonical_node::SchemaSource::RefUri(uri),
        }
    }

    fn convert_io_schema(schema: runtime::node::IoSchema) -> canonical_node::IoSchema {
        canonical_node::IoSchema {
            schema: convert_schema_source(schema.schema),
            content_type: schema.content_type,
            schema_version: schema.schema_version,
        }
    }

    fn convert_example(example: runtime::node::Example) -> canonical_node::Example {
        canonical_node::Example {
            title: example.title,
            input_cbor: example.input_cbor,
            output_cbor: example.output_cbor,
        }
    }

    fn convert_op(op: runtime::node::Op) -> canonical_node::Op {
        canonical_node::Op {
            name: op.name,
            summary: op.summary,
            input: convert_io_schema(op.input),
            output: convert_io_schema(op.output),
            examples: op.examples.into_iter().map(convert_example).collect(),
        }
    }

    fn convert_schema_ref(schema: runtime::node::SchemaRef) -> canonical_node::SchemaRef {
        canonical_node::SchemaRef {
            id: schema.id,
            content_type: schema.content_type,
            blake3_hash: schema.blake3_hash,
            version: schema.version,
            bytes: schema.bytes,
            uri: schema.uri,
        }
    }

    fn convert_setup_example(example: runtime::node::SetupExample) -> canonical_node::SetupExample {
        canonical_node::SetupExample {
            title: example.title,
            answers_cbor: example.answers_cbor,
        }
    }

    fn convert_setup_output(output: runtime::node::SetupOutput) -> canonical_node::SetupOutput {
        match output {
            runtime::node::SetupOutput::ConfigOnly => canonical_node::SetupOutput::ConfigOnly,
            runtime::node::SetupOutput::TemplateScaffold(scaffold) => {
                canonical_node::SetupOutput::TemplateScaffold(
                    canonical_node::SetupTemplateScaffold {
                        template_ref: scaffold.template_ref,
                        output_layout: scaffold.output_layout,
                    },
                )
            }
        }
    }

    fn convert_setup_contract(
        contract: runtime::node::SetupContract,
    ) -> canonical_node::SetupContract {
        canonical_node::SetupContract {
            qa_spec: convert_schema_source(contract.qa_spec),
            answers_schema: convert_schema_source(contract.answers_schema),
            examples: contract
                .examples
                .into_iter()
                .map(convert_setup_example)
                .collect(),
            outputs: contract
                .outputs
                .into_iter()
                .map(convert_setup_output)
                .collect(),
        }
    }

    fn convert_descriptor(descriptor: runtime::node::ComponentDescriptor) -> ComponentDescriptor {
        ComponentDescriptor {
            name: descriptor.name,
            version: descriptor.version,
            summary: descriptor.summary,
            capabilities: descriptor.capabilities,
            ops: descriptor.ops.into_iter().map(convert_op).collect(),
            schemas: descriptor
                .schemas
                .into_iter()
                .map(convert_schema_ref)
                .collect(),
            setup: descriptor.setup.map(convert_setup_contract),
        }
    }

    pub(super) fn setup_apply_payload(
        mode: WizardMode,
        current_config: &[u8],
        answers: &[u8],
    ) -> Result<Vec<u8>> {
        use ciborium::value::Value as CValue;

        let current = if matches!(mode, WizardMode::Update | WizardMode::Remove) {
            CValue::Bytes(current_config.to_vec())
        } else {
            CValue::Null
        };
        let answers_value = if matches!(
            mode,
            WizardMode::Default | WizardMode::Setup | WizardMode::Update
        ) {
            CValue::Bytes(answers.to_vec())
        } else {
            CValue::Null
        };

        let value = CValue::Map(vec![
            (
                CValue::Text("mode".to_string()),
                CValue::Text(mode.as_str().to_string()),
            ),
            (CValue::Text("current_config_cbor".to_string()), current),
            (CValue::Text("answers_cbor".to_string()), answers_value),
            (CValue::Text("metadata_cbor".to_string()), CValue::Null),
        ]);

        let mut out = Vec::new();
        ciborium::ser::into_writer(&value, &mut out)
            .map_err(|err| anyhow!("encode setup.apply_answers payload: {err}"))?;
        Ok(out)
    }

    fn invoke_setup_apply(
        wasm_bytes: &[u8],
        mode: WizardMode,
        current_config: &[u8],
        answers: &[u8],
    ) -> Result<Vec<u8>> {
        let engine = wizard_engine()?;
        let component = load_component_cached(wasm_bytes)?;
        let mut linker: Linker<HostState> = Linker::new(engine);
        add_wasi_imports(&mut linker)?;
        add_control_imports(&mut linker)?;
        let mut store = Store::new(engine, HostState::new());
        let api = runtime::RuntimeComponent::instantiate(&mut store, component.as_ref(), &linker)
            .map_err(|err| anyhow!("instantiate canonical component world: {err}"))?;
        let node = api.greentic_component_node();

        let payload_cbor = setup_apply_payload(mode, current_config, answers)?;
        let envelope = invoke_envelope(payload_cbor);
        let result = node
            .call_invoke(&mut store, "setup.apply_answers", &envelope)
            .map_err(|err| anyhow!("call invoke(setup.apply_answers): {err}"))?;

        let runtime::node::InvocationResult {
            ok,
            output_cbor,
            output_metadata_cbor: _,
        } = result.map_err(|err| anyhow!("invoke returned node error: {}", err.message))?;

        if !ok {
            return Err(anyhow!(
                "invoke(setup.apply_answers) returned ok=false with no node error"
            ));
        }

        Ok(output_cbor)
    }

    pub(super) fn descriptor_mode_name(mode: WizardMode) -> &'static str {
        match mode {
            WizardMode::Default => "default",
            WizardMode::Setup => "setup",
            WizardMode::Update => "update",
            WizardMode::Remove => "remove",
        }
    }

    pub(super) fn is_missing_node_instance_error(err: &anyhow::Error) -> bool {
        format!("{err:#}").contains("no exported instance named `greentic:component/node@0.6.0`")
    }

    pub(super) fn is_missing_setup_contract_error(err: &anyhow::Error) -> bool {
        let msg = format!("{err:#}");
        msg.contains("component descriptor missing setup.qa-spec")
            || msg.contains(
                "component descriptor does not advertise required op 'setup.apply_answers'",
            )
    }

    pub(super) fn is_missing_setup_apply_error(err: &anyhow::Error) -> bool {
        format!("{err:#}").contains("setup.apply_answers")
    }

    fn instantiate_root(
        wasm_bytes: &[u8],
        add_control: bool,
    ) -> Result<(Store<HostState>, wasmtime::component::Instance)> {
        let engine = wizard_engine()?;
        let component = load_component_cached(wasm_bytes)?;
        let mut linker: Linker<HostState> = Linker::new(engine);
        add_wasi_imports(&mut linker)?;
        if add_control {
            add_control_imports(&mut linker)?;
        }
        let mut store = Store::new(engine, HostState::new());
        let instance = linker
            .instantiate(&mut store, component.as_ref())
            .map_err(|err| anyhow!("instantiate component root world: {err}"))?;
        Ok((store, instance))
    }

    fn find_export_index(
        store: &mut Store<HostState>,
        instance: &wasmtime::component::Instance,
        parent: Option<&wasmtime::component::ComponentExportIndex>,
        names: &[&str],
    ) -> Option<wasmtime::component::ComponentExportIndex> {
        for name in names {
            if let Some(index) = instance.get_export_index(&mut *store, parent, name) {
                return Some(index);
            }
        }
        None
    }

    fn fetch_descriptor_spec(wasm_bytes: &[u8], mode: WizardMode) -> Result<WizardSpecOutput> {
        let (mut store, instance) = instantiate_root(wasm_bytes, false)?;
        let descriptor_instance = find_export_index(
            &mut store,
            &instance,
            None,
            &[
                "component-descriptor",
                "greentic:component/component-descriptor",
                "greentic:component/component-descriptor@0.6.0",
            ],
        );
        let describe_cbor = if let Some(descriptor_instance) = descriptor_instance {
            let describe_export = find_export_index(
                &mut store,
                &instance,
                Some(&descriptor_instance),
                &[
                    "describe",
                    "greentic:component/component-descriptor@0.6.0#describe",
                ],
            );
            if let Some(describe_export) = describe_export {
                let describe_func = instance
                    .get_typed_func::<(), (Vec<u8>,)>(&mut store, &describe_export)
                    .map_err(|err| anyhow!("lookup component-descriptor.describe: {err}"))?;
                let (describe_cbor,) = describe_func
                    .call(&mut store, ())
                    .map_err(|err| anyhow!("call component-descriptor.describe: {err}"))?;
                describe_cbor
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        let qa_instance = find_export_index(
            &mut store,
            &instance,
            None,
            &[
                "component-qa",
                "greentic:component/component-qa",
                "greentic:component/component-qa@0.6.0",
            ],
        )
        .ok_or_else(|| anyhow!("missing exported component-qa instance"))?;
        let qa_spec_export = find_export_index(
            &mut store,
            &instance,
            Some(&qa_instance),
            &["qa-spec", "greentic:component/component-qa@0.6.0#qa-spec"],
        )
        .ok_or_else(|| anyhow!("missing exported component-qa.qa-spec function"))?;
        let qa_spec_cbor = call_exported_bytes(
            &mut store,
            &instance,
            &qa_spec_export,
            &[Val::Enum(descriptor_mode_name(mode).to_string())],
            "component-qa.qa-spec",
        )?;

        Ok(WizardSpecOutput {
            abi: WizardAbi::V6,
            describe_cbor,
            descriptor: None,
            qa_spec_cbor,
            answers_schema_cbor: None,
        })
    }

    fn apply_descriptor_answers(
        wasm_bytes: &[u8],
        mode: WizardMode,
        current_config: &[u8],
        answers: &[u8],
    ) -> Result<Vec<u8>> {
        let (mut store, instance) = instantiate_root(wasm_bytes, false)?;
        let qa_instance = find_export_index(
            &mut store,
            &instance,
            None,
            &[
                "component-qa",
                "greentic:component/component-qa",
                "greentic:component/component-qa@0.6.0",
            ],
        )
        .ok_or_else(|| anyhow!("missing exported component-qa instance"))?;
        let apply_export = find_export_index(
            &mut store,
            &instance,
            Some(&qa_instance),
            &[
                "apply-answers",
                "greentic:component/component-qa@0.6.0#apply-answers",
            ],
        )
        .ok_or_else(|| anyhow!("missing exported component-qa.apply-answers function"))?;
        call_exported_bytes(
            &mut store,
            &instance,
            &apply_export,
            &[
                Val::Enum(descriptor_mode_name(mode).to_string()),
                bytes_to_val(current_config),
                bytes_to_val(answers),
            ],
            "component-qa.apply-answers",
        )
    }

    pub(super) fn bytes_to_val(bytes: &[u8]) -> Val {
        Val::List(bytes.iter().copied().map(Val::U8).collect())
    }

    pub(super) fn val_to_bytes(value: &Val) -> Result<Vec<u8>> {
        match value {
            Val::List(values) => values
                .iter()
                .map(|value| match value {
                    Val::U8(byte) => Ok(*byte),
                    other => Err(anyhow!("expected list<u8> item, got {other:?}")),
                })
                .collect(),
            other => Err(anyhow!("expected list<u8> result, got {other:?}")),
        }
    }

    fn call_exported_bytes(
        store: &mut Store<HostState>,
        instance: &wasmtime::component::Instance,
        export: &wasmtime::component::ComponentExportIndex,
        params: &[Val],
        label: &str,
    ) -> Result<Vec<u8>> {
        let func = instance
            .get_func(&mut *store, export)
            .ok_or_else(|| anyhow!("lookup {label}: function export not found"))?;
        let mut results = [Val::Bool(false)];
        func.call(&mut *store, params, &mut results)
            .map_err(|err| anyhow!("call {label}: {err}"))?;
        val_to_bytes(&results[0]).map_err(|err| anyhow!("{label} returned invalid bytes: {err}"))
    }

    pub fn fetch_wizard_spec(wasm_bytes: &[u8], _mode: WizardMode) -> Result<WizardSpecOutput> {
        let engine = wizard_engine()?;
        let component = load_component_cached(wasm_bytes)?;
        let mut linker: Linker<HostState> = Linker::new(engine);
        add_wasi_imports(&mut linker)?;
        add_control_imports(&mut linker)?;
        let mut store = Store::new(engine, HostState::new());
        let api =
            match runtime::RuntimeComponent::instantiate(&mut store, component.as_ref(), &linker) {
                Ok(api) => api,
                Err(err) => {
                    let err = anyhow!("instantiate canonical component world: {err}");
                    if is_missing_node_instance_error(&err) {
                        return fetch_descriptor_spec(wasm_bytes, _mode);
                    }
                    return Err(err);
                }
            };
        let node = api.greentic_component_node();

        let descriptor = node
            .call_describe(&mut store)
            .map(convert_descriptor)
            .map_err(|err| anyhow!("call describe: {err}"))?;
        let (qa_spec_cbor, answers_schema_cbor) = match extract_setup_contract(&descriptor)
            .and_then(|(qa_spec_cbor, answers_schema_cbor)| {
                ensure_setup_apply_answers_op(&descriptor)?;
                Ok((qa_spec_cbor, answers_schema_cbor))
            }) {
            Ok(values) => values,
            Err(err) if is_missing_setup_contract_error(&err) => {
                return fetch_descriptor_spec(wasm_bytes, _mode);
            }
            Err(err) => return Err(err),
        };

        Ok(WizardSpecOutput {
            abi: WizardAbi::V6,
            describe_cbor: Vec::new(),
            descriptor: Some(descriptor),
            qa_spec_cbor,
            answers_schema_cbor,
        })
    }

    pub fn apply_wizard_answers(
        wasm_bytes: &[u8],
        _abi: WizardAbi,
        mode: WizardMode,
        current_config: &[u8],
        answers: &[u8],
    ) -> Result<Vec<u8>> {
        match invoke_setup_apply(wasm_bytes, mode, current_config, answers) {
            Ok(config) if setup_apply_result_requires_descriptor_fallback(&config) => {
                apply_descriptor_answers(wasm_bytes, mode, current_config, answers)
            }
            Ok(config) => Ok(config),
            Err(err)
                if is_missing_node_instance_error(&err)
                    || is_missing_setup_apply_error(&err)
                    || is_setup_apply_descriptor_fallback_error(&err) =>
            {
                apply_descriptor_answers(wasm_bytes, mode, current_config, answers)
            }
            Err(err) => Err(err),
        }
    }

    pub(super) fn is_setup_apply_descriptor_fallback_error(err: &anyhow::Error) -> bool {
        let message = err.to_string().to_ascii_lowercase();
        message.contains("ac_schema_invalid")
            || message.contains("failed to decode cbor")
            || message.contains("decode cbor")
            || message.contains("invalid type: byte array, expected any valid json value")
            || (message.contains("byte array") && message.contains("expected any valid json value"))
    }

    pub(super) fn setup_apply_result_requires_descriptor_fallback(config_cbor: &[u8]) -> bool {
        let Ok(config_json) = super::cbor_to_json(config_cbor) else {
            return false;
        };
        let Some(error) = config_json.get("error").and_then(|value| value.as_object()) else {
            return false;
        };

        let mut combined = String::new();
        if let Some(code) = error.get("code").and_then(|value| value.as_str()) {
            combined.push_str(code);
            combined.push(' ');
        }
        if let Some(message) = error.get("message").and_then(|value| value.as_str()) {
            combined.push_str(message);
            combined.push(' ');
        }
        if let Some(details) = error.get("details").and_then(|value| value.as_str()) {
            combined.push_str(details);
        }

        is_setup_apply_descriptor_fallback_error(&anyhow::anyhow!(combined))
    }

    #[cfg(test)]
    pub(super) fn wizard_cache_metrics() -> Result<crate::cache::CacheMetricsSnapshot> {
        Ok(wizard_runtime_cache()?.component_cache.metrics())
    }

    #[cfg(test)]
    pub(super) fn load_cached_component_for_tests(wasm_bytes: &[u8]) -> Result<()> {
        let _ = load_component_cached(wasm_bytes)?;
        Ok(())
    }

    #[cfg(test)]
    mod host_helper_tests {
        use super::*;

        #[test]
        fn schema_source_to_cbor_accepts_inline_and_rejects_references() {
            assert_eq!(
                schema_source_to_cbor(&SchemaSource::InlineCbor(vec![1, 2]), "qa-spec").unwrap(),
                vec![1, 2]
            );
            assert!(schema_source_to_cbor(&SchemaSource::CborSchemaId("schema-id".into()), "qa")
                .is_err());
            assert!(schema_source_to_cbor(&SchemaSource::RefPackPath("pack/path".into()), "qa")
                .is_err());
            assert!(schema_source_to_cbor(&SchemaSource::RefUri("https://example.invalid".into()), "qa")
                .is_err());
        }

        #[test]
        fn convert_runtime_descriptor_helpers_preserve_fields() {
            let io_schema = runtime::node::IoSchema {
                schema: runtime::node::SchemaSource::CborSchemaId("input-schema".to_string()),
                content_type: "application/cbor".to_string(),
                schema_version: Some("1".to_string()),
            };
            let example = runtime::node::Example {
                title: "demo".to_string(),
                input_cbor: vec![4],
                output_cbor: vec![5],
            };
            let op = runtime::node::Op {
                name: "run".to_string(),
                summary: Some("summary".to_string()),
                input: io_schema.clone(),
                output: runtime::node::IoSchema {
                    schema: runtime::node::SchemaSource::RefPackPath("schemas/output.cbor".into()),
                    content_type: "application/cbor".to_string(),
                    schema_version: Some("2".to_string()),
                },
                examples: vec![example],
            };
            let schema_ref = runtime::node::SchemaRef {
                id: "schema-id".to_string(),
                content_type: "application/json".to_string(),
                blake3_hash: "hash".to_string(),
                version: "1".to_string(),
                bytes: Some(vec![9]),
                uri: Some("https://example.invalid/schema".to_string()),
            };
            let setup = runtime::node::SetupContract {
                qa_spec: runtime::node::SchemaSource::RefUri(
                    "https://example.invalid/qa-spec".to_string(),
                ),
                answers_schema: runtime::node::SchemaSource::InlineCbor(vec![7]),
                examples: vec![runtime::node::SetupExample {
                    title: "setup".to_string(),
                    answers_cbor: vec![8],
                }],
                outputs: vec![
                    runtime::node::SetupOutput::ConfigOnly,
                    runtime::node::SetupOutput::TemplateScaffold(
                        runtime::node::SetupTemplateScaffold {
                            template_ref: "template".to_string(),
                            output_layout: Some("layout".to_string()),
                        },
                    ),
                ],
            };

            let descriptor = convert_descriptor(runtime::node::ComponentDescriptor {
                name: "component".to_string(),
                version: "0.1.0".to_string(),
                summary: Some("summary".to_string()),
                capabilities: vec!["http".to_string()],
                ops: vec![op],
                schemas: vec![schema_ref],
                setup: Some(setup),
            });

            assert_eq!(descriptor.name, "component");
            assert_eq!(descriptor.ops[0].name, "run");
            assert_eq!(descriptor.ops[0].examples.len(), 1);
            assert!(matches!(
                descriptor.ops[0].input.schema,
                canonical_node::SchemaSource::CborSchemaId(ref id) if id == "input-schema"
            ));
            assert!(matches!(
                descriptor.ops[0].output.schema,
                canonical_node::SchemaSource::RefPackPath(ref path) if path == "schemas/output.cbor"
            ));
            assert_eq!(descriptor.schemas[0].id, "schema-id");
            assert_eq!(descriptor.setup.as_ref().unwrap().examples.len(), 1);
            assert_eq!(descriptor.setup.as_ref().unwrap().outputs.len(), 2);
            assert!(matches!(
                descriptor.setup.as_ref().unwrap().qa_spec,
                canonical_node::SchemaSource::RefUri(ref uri)
                    if uri == "https://example.invalid/qa-spec"
            ));
            assert!(matches!(
                descriptor.setup.as_ref().unwrap().answers_schema,
                canonical_node::SchemaSource::InlineCbor(ref bytes) if bytes == &vec![7]
            ));
        }

        #[test]
        fn invoke_envelope_sets_local_context_defaults() {
            let envelope = invoke_envelope(vec![1, 2, 3]);
            assert_eq!(envelope.flow_id, "wizard-flow");
            assert_eq!(envelope.step_id, "wizard-step");
            assert_eq!(envelope.payload_cbor, vec![1, 2, 3]);
            assert_eq!(envelope.ctx.tenant_id, "local");
            assert_eq!(envelope.ctx.i18n_id, "en-US");
        }

        #[test]
        fn setup_payload_and_error_helpers_cover_null_and_negative_paths() {
            let payload = setup_apply_payload(super::super::WizardMode::Remove, &[0xaa], &[0xbb])
                .expect("payload");
            let decoded: ciborium::value::Value =
                ciborium::de::from_reader(payload.as_slice()).expect("decode payload");
            let ciborium::value::Value::Map(entries) = decoded else {
                panic!("expected cbor map");
            };
            assert!(entries.iter().any(|(key, value)| {
                matches!(key, ciborium::value::Value::Text(text) if text == "answers_cbor")
                    && matches!(value, ciborium::value::Value::Null)
            }));

            let invalid_list = wasmtime::component::Val::List(vec![wasmtime::component::Val::Bool(true)]);
            assert!(val_to_bytes(&invalid_list).is_err());
            assert!(!is_missing_node_instance_error(&anyhow::anyhow!("different error")));
            assert!(!is_missing_setup_apply_error(&anyhow::anyhow!("different error")));
            assert!(!is_missing_setup_contract_error(&anyhow::anyhow!("different error")));
        }

        #[test]
        fn setup_payload_default_mode_omits_current_config_but_keeps_answers() {
            let payload = setup_apply_payload(super::super::WizardMode::Default, &[0xaa], &[0xbb])
                .expect("payload");
            let decoded: ciborium::value::Value =
                ciborium::de::from_reader(payload.as_slice()).expect("decode payload");
            let ciborium::value::Value::Map(entries) = decoded else {
                panic!("expected cbor map");
            };
            assert!(entries.iter().any(|(key, value)| {
                matches!(key, ciborium::value::Value::Text(text) if text == "current_config_cbor")
                    && matches!(value, ciborium::value::Value::Null)
            }));
            assert!(entries.iter().any(|(key, value)| {
                matches!(key, ciborium::value::Value::Text(text) if text == "answers_cbor")
                    && matches!(value, ciborium::value::Value::Bytes(bytes) if bytes == &vec![0xbb])
            }));
        }
    }

    pub fn run_wizard_ops(
        wasm_bytes: &[u8],
        mode: WizardMode,
        current_config: &[u8],
        answers: &[u8],
    ) -> Result<WizardOutput> {
        let spec = fetch_wizard_spec(wasm_bytes, mode)?;
        let config_cbor =
            apply_wizard_answers(wasm_bytes, spec.abi, mode, current_config, answers)?;
        Ok(WizardOutput {
            abi: spec.abi,
            describe_cbor: spec.describe_cbor,
            descriptor: spec.descriptor,
            qa_spec_cbor: spec.qa_spec_cbor,
            answers_cbor: answers.to_vec(),
            config_cbor,
        })
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use host::{apply_wizard_answers, fetch_wizard_spec, run_wizard_ops};

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use ciborium::value::Value as CValue;
    use greentic_distributor_client::{CachePolicy, DistClient, ResolvePolicy};
    use greentic_interfaces_host::component_v0_6::exports::greentic::component::node::{
        ComponentDescriptor, IoSchema, Op, SchemaSource, SetupContract,
    };
    use serde::Deserialize;

    fn adaptive_card_wasm_bytes() -> Option<Vec<u8>> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../component-adaptive-card/dist/component_adaptive_card__0_6_0.wasm");
        if !path.exists() {
            return None;
        }
        Some(fs::read(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display())))
    }

    #[derive(Deserialize)]
    struct FrequentComponentEntry {
        id: String,
        component_ref: String,
    }

    #[derive(Deserialize)]
    struct FrequentComponentsCatalog {
        components: Vec<FrequentComponentEntry>,
    }

    fn frequent_component_ref(id: &str) -> Option<String> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("frequent-components.json");
        let raw = fs::read_to_string(&path).ok()?;
        let catalog: FrequentComponentsCatalog = serde_json::from_str(&raw).ok()?;
        catalog
            .components
            .into_iter()
            .find(|entry| entry.id == id)
            .map(|entry| entry.component_ref)
    }

    fn frequent_component_wasm_bytes(id: &str) -> Option<Vec<u8>> {
        let reference = frequent_component_ref(id)?;
        let runtime = tokio::runtime::Runtime::new().ok()?;
        let client = DistClient::new(Default::default());
        let source = client.parse_source(&reference).ok()?;
        let descriptor = runtime
            .block_on(client.resolve(source, ResolvePolicy))
            .ok()?;
        let resolved = runtime
            .block_on(client.fetch(&descriptor, CachePolicy))
            .ok()?;
        let cache_path = resolved.cache_path?;
        let bytes = fs::read(&cache_path).ok()?;
        if bytes.is_empty() {
            return None;
        }
        Some(bytes)
    }

    #[test]
    fn setup_apply_fallback_classifier_matches_json_byte_array_error() {
        let err = anyhow::anyhow!(
            "AC_SCHEMA_INVALID: invalid type: byte array, expected any valid JSON value"
        );
        assert!(super::host::is_setup_apply_descriptor_fallback_error(&err));
    }

    #[test]
    fn setup_apply_fallback_classifier_matches_decode_cbor_error() {
        let err = anyhow::anyhow!("call invoke: failed to decode cbor payload");
        assert!(super::host::is_setup_apply_descriptor_fallback_error(&err));
    }

    #[test]
    fn setup_apply_fallback_classifier_ignores_unrelated_errors() {
        let err = anyhow::anyhow!("call invoke: permission denied");
        assert!(!super::host::is_setup_apply_descriptor_fallback_error(&err));
    }

    #[test]
    fn setup_apply_fallback_classifier_matches_error_payload_cbor() {
        let payload = serde_json::json!({
            "error": {
                "code": "AC_SCHEMA_INVALID",
                "message": "Invalid CBOR invocation",
                "details": "invalid input: failed to decode cbor: CBOR decode failed: Semantic(None, \"invalid type: byte array, expected any valid JSON value\")"
            }
        });
        let cbor = super::json_to_cbor(&payload).expect("payload cbor");
        assert!(super::host::setup_apply_result_requires_descriptor_fallback(&cbor));
    }

    #[test]
    fn setup_apply_result_fallback_classifier_ignores_normal_payloads_and_garbage() {
        let ok_payload = super::json_to_cbor(&serde_json::json!({ "ok": true })).unwrap();
        assert!(!super::host::setup_apply_result_requires_descriptor_fallback(&ok_payload));
        assert!(!super::host::setup_apply_result_requires_descriptor_fallback(b"not-cbor"));
    }

    #[test]
    fn setup_contract_helpers_require_inline_cbor_and_setup_apply_op() {
        let descriptor = ComponentDescriptor {
            name: "component".to_string(),
            version: "0.1.0".to_string(),
            summary: None,
            capabilities: Vec::new(),
            ops: vec![Op {
                name: "setup.apply_answers".to_string(),
                summary: None,
                input: IoSchema {
                    schema: SchemaSource::InlineCbor(vec![1]),
                    content_type: "application/cbor".to_string(),
                    schema_version: None,
                },
                output: IoSchema {
                    schema: SchemaSource::InlineCbor(vec![2]),
                    content_type: "application/cbor".to_string(),
                    schema_version: None,
                },
                examples: Vec::new(),
            }],
            schemas: Vec::new(),
            setup: Some(SetupContract {
                qa_spec: SchemaSource::InlineCbor(vec![1, 2, 3]),
                answers_schema: SchemaSource::InlineCbor(vec![4, 5, 6]),
                examples: Vec::new(),
                outputs: Vec::new(),
            }),
        };

        let (qa_spec, answers_schema) = super::host::extract_setup_contract(&descriptor).unwrap();
        assert_eq!(qa_spec, vec![1, 2, 3]);
        assert_eq!(answers_schema, Some(vec![4, 5, 6]));
        super::host::ensure_setup_apply_answers_op(&descriptor).unwrap();

        let bad_descriptor = ComponentDescriptor {
            setup: Some(SetupContract {
                qa_spec: SchemaSource::RefUri("https://example.invalid/schema".to_string()),
                answers_schema: SchemaSource::InlineCbor(vec![1]),
                examples: Vec::new(),
                outputs: Vec::new(),
            }),
            ops: Vec::new(),
            ..descriptor
        };
        assert!(super::host::extract_setup_contract(&bad_descriptor).is_err());
        assert!(super::host::ensure_setup_apply_answers_op(&bad_descriptor).is_err());
    }

    #[test]
    fn setup_payload_and_byte_value_helpers_encode_expected_shapes() {
        let payload = super::host::setup_apply_payload(
            super::WizardMode::Update,
            &[0xaa],
            &[0xbb, 0xcc],
        )
        .expect("payload");
        let decoded: CValue = ciborium::de::from_reader(payload.as_slice()).expect("decode payload");
        let CValue::Map(entries) = decoded else {
            panic!("expected cbor map");
        };
        assert!(entries.iter().any(|(key, value)| {
            matches!(key, CValue::Text(text) if text == "mode")
                && matches!(value, CValue::Text(mode) if mode == "update")
        }));
        assert!(entries.iter().any(|(key, value)| {
            matches!(key, CValue::Text(text) if text == "answers_cbor")
                && matches!(value, CValue::Bytes(bytes) if bytes == &vec![0xbb, 0xcc])
        }));

        let val = super::host::bytes_to_val(&[1, 2, 3]);
        assert_eq!(super::host::val_to_bytes(&val).unwrap(), vec![1, 2, 3]);
        let err = super::host::val_to_bytes(&wasmtime::component::Val::Bool(true))
            .expect_err("non-byte list should fail");
        assert!(format!("{err}").contains("expected list<u8> result"));
    }

    #[test]
    fn wizard_host_error_classifiers_match_expected_messages() {
        assert_eq!(super::host::descriptor_mode_name(super::WizardMode::Default), "default");
        assert_eq!(super::host::descriptor_mode_name(super::WizardMode::Setup), "setup");
        assert_eq!(super::host::descriptor_mode_name(super::WizardMode::Update), "update");
        assert_eq!(super::host::descriptor_mode_name(super::WizardMode::Remove), "remove");
        assert!(super::host::is_missing_node_instance_error(&anyhow::anyhow!(
            "no exported instance named `greentic:component/node@0.6.0`"
        )));
        assert!(super::host::is_missing_setup_contract_error(&anyhow::anyhow!(
            "component descriptor missing setup.qa-spec"
        )));
        assert!(super::host::is_missing_setup_apply_error(&anyhow::anyhow!(
            "missing setup.apply_answers function"
        )));
    }

    #[test]
    fn wizard_mode_maps_to_expected_strings_and_qa_modes() {
        assert_eq!(super::WizardMode::Default.as_str(), "default");
        assert_eq!(super::WizardMode::Setup.as_str(), "setup");
        assert_eq!(super::WizardMode::Update.as_str(), "update");
        assert_eq!(super::WizardMode::Remove.as_str(), "remove");

        assert_eq!(super::WizardMode::Default.as_qa_mode(), greentic_types::schemas::component::v0_6_0::QaMode::Default);
        assert_eq!(super::WizardMode::Setup.as_qa_mode(), greentic_types::schemas::component::v0_6_0::QaMode::Setup);
        assert_eq!(super::WizardMode::Update.as_qa_mode(), greentic_types::schemas::component::v0_6_0::QaMode::Update);
        assert_eq!(super::WizardMode::Remove.as_qa_mode(), greentic_types::schemas::component::v0_6_0::QaMode::Remove);
    }

    #[test]
    fn wizard_component_cache_reuses_compiled_artifact() {
        let Some(wasm_bytes) = adaptive_card_wasm_bytes() else {
            return;
        };

        super::host::load_cached_component_for_tests(&wasm_bytes).expect("first cached load");
        let after_first = super::host::wizard_cache_metrics().expect("first metrics");

        super::host::load_cached_component_for_tests(&wasm_bytes).expect("second cached load");
        let after_second = super::host::wizard_cache_metrics().expect("second metrics");

        assert_eq!(
            after_second.compiles, after_first.compiles,
            "second load should not trigger another compile"
        );
        assert!(
            after_second.memory_hits > after_first.memory_hits
                || after_second.disk_hits > after_first.disk_hits,
            "second load should hit memory or disk cache"
        );
    }

    #[test]
    fn frequent_http_component_no_longer_fails_with_missing_http_linker_import() {
        let Some(wasm_bytes) = frequent_component_wasm_bytes("http") else {
            return;
        };

        let message = match super::host::fetch_wizard_spec(&wasm_bytes, super::WizardMode::Default)
        {
            Ok(_) => return,
            Err(err) => format!("{err:#}"),
        };
        assert!(
            !message.contains("matching implementation was not found in the linker"),
            "expected greentic-flow host linker fix to be active, got: {message}"
        );
        assert!(
            !message.contains("greentic:http/http-client@1.1.0"),
            "expected http client host import to be linked, got: {message}"
        );
    }

    #[test]
    fn frequent_llm_component_no_longer_fails_with_missing_linker_implementations() {
        let Some(wasm_bytes) = frequent_component_wasm_bytes("llm-openai") else {
            return;
        };

        let message = match super::host::fetch_wizard_spec(&wasm_bytes, super::WizardMode::Default)
        {
            Ok(_) => return,
            Err(err) => format!("{err:#}"),
        };
        assert!(
            !message.contains("matching implementation was not found in the linker"),
            "expected greentic-flow host linker fix to be active, got: {message}"
        );
    }

    #[test]
    fn public_wizard_entrypoints_fail_cleanly_for_invalid_component_bytes() {
        assert!(super::fetch_wizard_spec(b"not-a-component", super::WizardMode::Default).is_err());
        assert!(
            super::apply_wizard_answers(
                b"not-a-component",
                super::WizardAbi::V6,
                super::WizardMode::Default,
                &[],
                &[],
            )
            .is_err()
        );
        assert!(super::run_wizard_ops(b"not-a-component", super::WizardMode::Default, &[], &[])
            .is_err());
    }
}

#[cfg(target_arch = "wasm32")]
pub fn run_wizard_ops(
    _wasm_bytes: &[u8],
    _mode: WizardMode,
    _current_config: &[u8],
    _answers: &[u8],
) -> Result<WizardOutput> {
    Err(anyhow!("setup ops not supported on wasm targets"))
}

pub fn decode_component_qa_spec(qa_spec_cbor: &[u8], mode: WizardMode) -> Result<ComponentQaSpec> {
    let decoded: Result<ComponentQaSpec> =
        canonical::from_cbor(qa_spec_cbor).map_err(|err| anyhow!("decode qa-spec cbor: {err}"));
    if let Ok(spec) = decoded {
        return Ok(spec);
    }

    let legacy_json = std::str::from_utf8(qa_spec_cbor)
        .ok()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    if let Some(raw) = legacy_json {
        let adapted =
            greentic_types::adapters::component_v0_5_0_to_v0_6_0::adapt_component_qa_spec_json(
                mode.as_qa_mode(),
                raw,
            )
            .map_err(|err| anyhow!("adapt legacy qa-spec json: {err}"))?;
        let spec: ComponentQaSpec = canonical::from_cbor(adapted.as_slice())
            .map_err(|err| anyhow!("decode adapted qa-spec: {err}"))?;
        return Ok(spec);
    }

    Err(anyhow!("qa-spec payload is not valid CBOR or legacy JSON"))
}

pub fn answers_to_cbor(answers: &HashMap<String, JsonValue>) -> Result<Vec<u8>> {
    let mut map = serde_json::Map::new();
    for (k, v) in answers {
        map.insert(k.clone(), v.clone());
    }
    let json = JsonValue::Object(map);
    let bytes = canonical::to_canonical_cbor(&json)
        .map_err(|err| anyhow!("encode answers as canonical cbor: {err}"))?;
    Ok(bytes)
}

pub fn json_to_cbor(value: &JsonValue) -> Result<Vec<u8>> {
    let bytes = canonical::to_canonical_cbor(value)
        .map_err(|err| anyhow!("encode json as canonical cbor: {err}"))?;
    Ok(bytes)
}

pub fn cbor_to_json(bytes: &[u8]) -> Result<JsonValue> {
    let value: ciborium::value::Value =
        ciborium::de::from_reader(bytes).map_err(|err| anyhow!("decode cbor: {err}"))?;
    cbor_value_to_json(&value)
}

pub fn cbor_value_to_json(value: &ciborium::value::Value) -> Result<JsonValue> {
    use ciborium::value::Value as CValue;
    Ok(match value {
        CValue::Null => JsonValue::Null,
        CValue::Bool(b) => JsonValue::Bool(*b),
        CValue::Integer(i) => {
            if let Ok(v) = i64::try_from(*i) {
                JsonValue::Number(v.into())
            } else {
                let wide: i128 = (*i).into();
                JsonValue::String(wide.to_string())
            }
        }
        CValue::Float(f) => {
            let num = serde_json::Number::from_f64(*f)
                .ok_or_else(|| anyhow!("float out of range for json"))?;
            JsonValue::Number(num)
        }
        CValue::Text(s) => JsonValue::String(s.clone()),
        CValue::Bytes(b) => {
            JsonValue::Array(b.iter().map(|v| JsonValue::Number((*v).into())).collect())
        }
        CValue::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(cbor_value_to_json(item)?);
            }
            JsonValue::Array(out)
        }
        CValue::Map(entries) => {
            let mut map = serde_json::Map::new();
            for (k, v) in entries {
                let key = match k {
                    CValue::Text(s) => s.clone(),
                    other => return Err(anyhow!("non-string map key in cbor: {other:?}")),
                };
                map.insert(key, cbor_value_to_json(v)?);
            }
            JsonValue::Object(map)
        }
        CValue::Tag(_, inner) => cbor_value_to_json(inner)?,
        _ => return Err(anyhow!("unsupported cbor value")),
    })
}

pub fn qa_spec_to_questions(
    spec: &ComponentQaSpec,
    catalog: &I18nCatalog,
    locale: &str,
) -> Vec<crate::questions::Question> {
    let mut out = Vec::new();
    for question in &spec.questions {
        let prompt = resolve_text(&question.label, catalog, locale);
        let default = question
            .default
            .as_ref()
            .and_then(|value| cbor_value_to_json(value).ok());

        let (kind, choices) = match &question.kind {
            QuestionKind::Text => (crate::questions::QuestionKind::String, Vec::new()),
            QuestionKind::Number => (crate::questions::QuestionKind::Float, Vec::new()),
            QuestionKind::Bool => (crate::questions::QuestionKind::Bool, Vec::new()),
            QuestionKind::InlineJson { .. } => (crate::questions::QuestionKind::String, Vec::new()),
            QuestionKind::AssetRef { .. } => (crate::questions::QuestionKind::String, Vec::new()),
            QuestionKind::Choice { options } => {
                let mut values = Vec::new();
                for option in options {
                    values.push(JsonValue::String(option.value.clone()));
                }
                (crate::questions::QuestionKind::Choice, values)
            }
        };

        out.push(crate::questions::Question {
            id: question.id.clone(),
            prompt,
            kind,
            required: question.required,
            default,
            choices,
            show_if: None,
            writes_to: None,
        });
    }
    out
}

pub fn merge_default_answers(spec: &ComponentQaSpec, seed: &mut HashMap<String, JsonValue>) {
    for (key, value) in &spec.defaults {
        if seed.contains_key(key) {
            continue;
        }
        if let Ok(json_value) = cbor_value_to_json(value) {
            seed.insert(key.clone(), json_value);
        }
    }
}

pub fn ensure_answers_object(answers: &serde_json::Value) -> Result<()> {
    if matches!(answers, serde_json::Value::Object(_)) {
        return Ok(());
    }
    Err(anyhow!("answers must be a JSON object"))
}

pub fn empty_cbor_map() -> Vec<u8> {
    vec![0xa0]
}

pub fn describe_exports_for_meta(_abi: WizardAbi) -> Vec<String> {
    vec!["describe".to_string(), "invoke".to_string()]
}

pub fn abi_version_from_abi(_abi: WizardAbi) -> String {
    "0.6.0".to_string()
}

pub fn canonicalize_answers_map(answers: &serde_json::Map<String, JsonValue>) -> Result<Vec<u8>> {
    let mut map = BTreeMap::new();
    for (k, v) in answers {
        map.insert(k.clone(), v.clone());
    }
    let bytes =
        canonical::to_canonical_cbor(&map).map_err(|err| anyhow!("canonicalize answers: {err}"))?;
    Ok(bytes)
}
