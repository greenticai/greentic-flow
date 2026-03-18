pub mod v1;

// Back-compat re-exports for the first stabilized helper.
pub use v1::{
    HostFns, add_all_v1_to_linker as add_all_to_linker,
    http_client::add_http_client_to_linker,
    oauth_broker::add_oauth_broker_to_linker,
    runner_host_http::add_runner_host_http_to_linker,
    runner_host_kv::add_runner_host_kv_to_linker,
    secrets_store::{
        SecretsError, SecretsErrorV1_1, SecretsStoreHost, SecretsStoreHostV1_1,
        add_secrets_store_compat_to_linker, add_secrets_store_to_linker,
        add_secrets_store_v1_1_to_linker,
    },
    state_store::add_state_store_to_linker,
    telemetry_logger::add_telemetry_logger_to_linker,
};
