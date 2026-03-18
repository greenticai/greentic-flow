use crate::oauth_broker_broker_client_v1_0::greentic::oauth_broker::broker_v1 as bindings;

/// Host trait for `greentic:oauth-broker/broker-client@1.0.0`.
pub use bindings::Host as OAuthBrokerHost;

/// Register the OAuth broker world on the provided linker.
pub fn add_oauth_broker_to_linker<T>(
    linker: &mut wasmtime::component::Linker<T>,
    get: fn(&mut T) -> &mut dyn OAuthBrokerHost,
) -> anyhow::Result<()> {
    let mut instance = linker.instance("greentic:oauth-broker/broker-v1@1.0.0")?;
    instance.func_wrap(
        "get-consent-url",
        move |mut caller: wasmtime::StoreContextMut<'_, T>,
              (provider_id, subject, scopes, redirect_path, extra_json): (
            wasmtime::component::__internal::String,
            wasmtime::component::__internal::String,
            wasmtime::component::__internal::Vec<wasmtime::component::__internal::String>,
            wasmtime::component::__internal::String,
            wasmtime::component::__internal::String,
        )| {
            let host = get(caller.data_mut());
            let result =
                host.get_consent_url(provider_id, subject, scopes, redirect_path, extra_json);
            Ok((result,))
        },
    )?;
    instance.func_wrap(
        "exchange-code",
        move |mut caller: wasmtime::StoreContextMut<'_, T>,
              (provider_id, subject, code, redirect_path): (
            wasmtime::component::__internal::String,
            wasmtime::component::__internal::String,
            wasmtime::component::__internal::String,
            wasmtime::component::__internal::String,
        )| {
            let host = get(caller.data_mut());
            let result = host.exchange_code(provider_id, subject, code, redirect_path);
            Ok((result,))
        },
    )?;
    instance.func_wrap(
        "get-token",
        move |mut caller: wasmtime::StoreContextMut<'_, T>,
              (provider_id, subject, scopes): (
            wasmtime::component::__internal::String,
            wasmtime::component::__internal::String,
            wasmtime::component::__internal::Vec<wasmtime::component::__internal::String>,
        )| {
            let host = get(caller.data_mut());
            let result = host.get_token(provider_id, subject, scopes);
            Ok((result,))
        },
    )?;
    Ok(())
}
