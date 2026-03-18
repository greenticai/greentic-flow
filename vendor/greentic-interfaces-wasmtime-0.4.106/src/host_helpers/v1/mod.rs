//! Stable host-facing helpers for all v1 host-import worlds.
//!
//! Downstream hosts should implement the re-exported `Host` traits from these
//! modules and wire them via the `add_*_to_linker` helpers below instead of
//! depending on the generated module paths.

pub mod http_client;
pub mod oauth_broker;
pub mod runner_host_http;
pub mod runner_host_kv;
pub mod secrets_store;
pub mod state_store;
pub mod telemetry_logger;

/// Getters for all v1 host-import worlds.
///
/// Provide a closure for each world you want to wire; pass `None` to skip.
#[derive(Default)]
pub struct HostFns<T> {
    /// Prefer providing this to expose both `http-client@1.1.0` and the legacy `@1.0.0` import.
    pub http_client_v1_1: Option<fn(&mut T) -> &mut dyn http_client::HttpClientHostV1_1>,
    pub http_client: Option<fn(&mut T) -> &mut dyn http_client::HttpClientHost>,
    pub oauth_broker: Option<fn(&mut T) -> &mut dyn oauth_broker::OAuthBrokerHost>,
    pub runner_host_http: Option<fn(&mut T) -> &mut dyn runner_host_http::RunnerHostHttp>,
    pub runner_host_kv: Option<fn(&mut T) -> &mut dyn runner_host_kv::RunnerHostKv>,
    pub telemetry_logger: Option<fn(&mut T) -> &mut dyn telemetry_logger::TelemetryLoggerHost>,
    pub state_store: Option<fn(&mut T) -> &mut dyn state_store::StateStoreHost>,
    /// Prefer providing this to expose both `secrets-store@1.1.0` and the legacy `@1.0.0` import.
    pub secrets_store_v1_1: Option<fn(&mut T) -> &mut dyn secrets_store::SecretsStoreHostV1_1>,
    pub secrets_store: Option<fn(&mut T) -> &mut dyn secrets_store::SecretsStoreHost>,
}

/// Adds all provided v1 host-import worlds to the linker.
pub fn add_all_v1_to_linker<T>(
    linker: &mut wasmtime::component::Linker<T>,
    fns: HostFns<T>,
) -> anyhow::Result<()> {
    if let Some(get) = fns.http_client_v1_1 {
        http_client::add_http_client_compat_to_linker(linker, get)?;
    } else if let Some(get) = fns.http_client {
        http_client::add_http_client_to_linker(linker, get)?;
    }
    if let Some(get) = fns.oauth_broker {
        oauth_broker::add_oauth_broker_to_linker(linker, get)?;
    }
    if let Some(get) = fns.runner_host_http {
        runner_host_http::add_runner_host_http_to_linker(linker, get)?;
    }
    if let Some(get) = fns.runner_host_kv {
        runner_host_kv::add_runner_host_kv_to_linker(linker, get)?;
    }
    if let Some(get) = fns.telemetry_logger {
        telemetry_logger::add_telemetry_logger_to_linker(linker, get)?;
    }
    if let Some(get) = fns.state_store {
        state_store::add_state_store_to_linker(linker, get)?;
    }
    if let Some(get) = fns.secrets_store_v1_1 {
        secrets_store::add_secrets_store_compat_to_linker(linker, get)?;
    } else if let Some(get) = fns.secrets_store {
        secrets_store::add_secrets_store_to_linker(linker, get)?;
    }

    Ok(())
}
