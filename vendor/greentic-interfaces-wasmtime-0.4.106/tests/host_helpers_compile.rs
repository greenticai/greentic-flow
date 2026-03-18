use anyhow::Result;
use greentic_interfaces_wasmtime::host_helpers::v1::{
    HostFns, http_client, oauth_broker, runner_host_http, runner_host_kv, secrets_store,
    state_store, telemetry_logger,
};
use wasmtime::component::Linker;
use wasmtime::{Config, Engine};

struct DummyHttpClient;
impl http_client::HttpClientHost for DummyHttpClient {
    fn send(
        &mut self,
        _req: http_client::Request,
        _ctx: Option<http_client::TenantCtx>,
    ) -> std::result::Result<http_client::Response, http_client::HttpClientError> {
        Ok(http_client::Response {
            status: 200,
            headers: Vec::new(),
            body: None,
        })
    }
}

impl http_client::HttpClientHostV1_1 for DummyHttpClient {
    fn send(
        &mut self,
        _req: http_client::RequestV1_1,
        _opts: Option<http_client::RequestOptionsV1_1>,
        _ctx: Option<http_client::TenantCtxV1_1>,
    ) -> std::result::Result<http_client::ResponseV1_1, http_client::HttpClientErrorV1_1> {
        Ok(http_client::ResponseV1_1 {
            status: 200,
            headers: Vec::new(),
            body: None,
        })
    }
}

struct DummyOAuthBroker;
impl oauth_broker::OAuthBrokerHost for DummyOAuthBroker {
    fn get_consent_url(
        &mut self,
        _provider_id: wasmtime::component::__internal::String,
        _subject: wasmtime::component::__internal::String,
        _scopes: wasmtime::component::__internal::Vec<wasmtime::component::__internal::String>,
        _redirect_path: wasmtime::component::__internal::String,
        _extra_json: wasmtime::component::__internal::String,
    ) -> wasmtime::component::__internal::String {
        wasmtime::component::__internal::String::from("https://example.com")
    }

    fn exchange_code(
        &mut self,
        _provider_id: wasmtime::component::__internal::String,
        _subject: wasmtime::component::__internal::String,
        _code: wasmtime::component::__internal::String,
        _redirect_path: wasmtime::component::__internal::String,
    ) -> wasmtime::component::__internal::String {
        wasmtime::component::__internal::String::from("token")
    }

    fn get_token(
        &mut self,
        _provider_id: wasmtime::component::__internal::String,
        _subject: wasmtime::component::__internal::String,
        _scopes: wasmtime::component::__internal::Vec<wasmtime::component::__internal::String>,
    ) -> wasmtime::component::__internal::String {
        wasmtime::component::__internal::String::from("token")
    }
}

struct DummyRunnerHttp;
impl runner_host_http::RunnerHostHttp for DummyRunnerHttp {
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

struct DummyRunnerKv;
impl runner_host_kv::RunnerHostKv for DummyRunnerKv {
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

struct DummyTelemetry;
impl telemetry_logger::TelemetryLoggerHost for DummyTelemetry {
    fn log(
        &mut self,
        _span: telemetry_logger::SpanContext,
        _fields: wasmtime::component::__internal::Vec<(
            wasmtime::component::__internal::String,
            wasmtime::component::__internal::String,
        )>,
        _ctx: Option<telemetry_logger::TenantCtx>,
    ) -> std::result::Result<telemetry_logger::OpAck, telemetry_logger::TelemetryLoggerError> {
        Ok(telemetry_logger::OpAck::Ok)
    }
}

struct DummyStateStore;
impl state_store::StateStoreHost for DummyStateStore {
    fn read(
        &mut self,
        _key: state_store::StateKey,
        _ctx: Option<state_store::TenantCtx>,
    ) -> std::result::Result<wasmtime::component::__internal::Vec<u8>, state_store::StateStoreError>
    {
        Ok(Vec::new())
    }

    fn write(
        &mut self,
        _key: state_store::StateKey,
        _bytes: wasmtime::component::__internal::Vec<u8>,
        _ctx: Option<state_store::TenantCtx>,
    ) -> std::result::Result<state_store::OpAck, state_store::StateStoreError> {
        Ok(state_store::OpAck::Ok)
    }

    fn delete(
        &mut self,
        _key: state_store::StateKey,
        _ctx: Option<state_store::TenantCtx>,
    ) -> std::result::Result<state_store::OpAck, state_store::StateStoreError> {
        Ok(state_store::OpAck::Ok)
    }
}

struct DummySecrets;
impl secrets_store::SecretsStoreHost for DummySecrets {
    fn get(
        &mut self,
        _key: wasmtime::component::__internal::String,
    ) -> std::result::Result<
        Option<wasmtime::component::__internal::Vec<u8>>,
        secrets_store::SecretsError,
    > {
        Ok(None)
    }
}

impl secrets_store::SecretsStoreHostV1_1 for DummySecrets {
    fn get(
        &mut self,
        _key: wasmtime::component::__internal::String,
    ) -> std::result::Result<
        Option<wasmtime::component::__internal::Vec<u8>>,
        secrets_store::SecretsErrorV1_1,
    > {
        Ok(None)
    }

    fn put(
        &mut self,
        _key: wasmtime::component::__internal::String,
        _value: wasmtime::component::__internal::Vec<u8>,
    ) {
    }
}

struct HostState {
    http: DummyHttpClient,
    oauth: DummyOAuthBroker,
    runner_http: DummyRunnerHttp,
    runner_kv: DummyRunnerKv,
    telemetry: DummyTelemetry,
    state: DummyStateStore,
    secrets: DummySecrets,
}

#[test]
fn host_helpers_compile() -> Result<()> {
    let mut config = Config::new();
    config.wasm_component_model(true);
    let engine = Engine::new(&config)?;
    let mut linker: Linker<HostState> = Linker::new(&engine);

    http_client::add_http_client_to_linker(&mut linker, |state: &mut HostState| &mut state.http)?;
    oauth_broker::add_oauth_broker_to_linker(&mut linker, |state: &mut HostState| {
        &mut state.oauth
    })?;
    runner_host_http::add_runner_host_http_to_linker(&mut linker, |state: &mut HostState| {
        &mut state.runner_http
    })?;
    runner_host_kv::add_runner_host_kv_to_linker(&mut linker, |state: &mut HostState| {
        &mut state.runner_kv
    })?;
    telemetry_logger::add_telemetry_logger_to_linker(&mut linker, |state: &mut HostState| {
        &mut state.telemetry
    })?;
    state_store::add_state_store_to_linker(&mut linker, |state: &mut HostState| &mut state.state)?;
    secrets_store::add_secrets_store_to_linker(&mut linker, |state: &mut HostState| {
        &mut state.secrets
    })?;

    // Ensure compat helper wires both @1.1.0 and legacy @1.0.0 worlds without duplicate registration.
    let mut compat_linker: Linker<HostState> = Linker::new(&engine);
    http_client::add_http_client_compat_to_linker(&mut compat_linker, |state: &mut HostState| {
        &mut state.http
    })?;

    // Also ensure add_all works on a fresh linker without duplicating entries.
    let mut linker_all: Linker<HostState> = Linker::new(&engine);
    let fns = HostFns {
        http_client_v1_1: Some(|state: &mut HostState| &mut state.http),
        http_client: Some(|state: &mut HostState| &mut state.http),
        oauth_broker: Some(|state: &mut HostState| &mut state.oauth),
        runner_host_http: Some(|state: &mut HostState| &mut state.runner_http),
        runner_host_kv: Some(|state: &mut HostState| &mut state.runner_kv),
        telemetry_logger: Some(|state: &mut HostState| &mut state.telemetry),
        state_store: Some(|state: &mut HostState| &mut state.state),
        secrets_store_v1_1: Some(|state: &mut HostState| &mut state.secrets),
        secrets_store: Some(|state: &mut HostState| &mut state.secrets),
    };

    greentic_interfaces_wasmtime::host_helpers::v1::add_all_v1_to_linker(&mut linker_all, fns)?;

    Ok(())
}
