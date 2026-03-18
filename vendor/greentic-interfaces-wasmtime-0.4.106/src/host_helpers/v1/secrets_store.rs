use crate::secrets_store_store_v1_0::greentic::secrets_store::secrets_store as bindings_v1_0;
use crate::secrets_store_store_v1_1::greentic::secrets_store::secrets_store as bindings_v1_1;

/// Host trait for `greentic:secrets-store@1.0.0`.
pub use bindings_v1_0::Host as SecretsStoreHost;
pub use bindings_v1_0::SecretsError;

/// Host trait for `greentic:secrets-store@1.1.0`.
pub use bindings_v1_1::Host as SecretsStoreHostV1_1;
pub use bindings_v1_1::SecretsError as SecretsErrorV1_1;

/// Register the secrets-store world on the provided linker without exposing
/// generated module paths.
pub fn add_secrets_store_to_linker<T>(
    linker: &mut wasmtime::component::Linker<T>,
    get: fn(&mut T) -> &mut dyn SecretsStoreHost,
) -> anyhow::Result<()> {
    let mut instance = linker.instance("greentic:secrets-store/secrets-store@1.0.0")?;
    instance.func_wrap(
        "get",
        move |mut caller: wasmtime::StoreContextMut<'_, T>,
              (key,): (wasmtime::component::__internal::String,)| {
            let host = get(caller.data_mut());
            let result = host.get(key);
            Ok((result,))
        },
    )?;
    Ok(())
}

/// Register the secrets-store world v1.1.0 on the provided linker.
pub fn add_secrets_store_v1_1_to_linker<T>(
    linker: &mut wasmtime::component::Linker<T>,
    get: fn(&mut T) -> &mut dyn SecretsStoreHostV1_1,
) -> anyhow::Result<()> {
    let mut instance = linker.instance("greentic:secrets-store/secrets-store@1.1.0")?;
    instance.func_wrap(
        "get",
        move |mut caller: wasmtime::StoreContextMut<'_, T>,
              (key,): (wasmtime::component::__internal::String,)| {
            let host = get(caller.data_mut());
            let result = host.get(key);
            Ok((result,))
        },
    )?;
    instance.func_wrap(
        "put",
        move |mut caller: wasmtime::StoreContextMut<'_, T>,
              (key, value): (
            wasmtime::component::__internal::String,
            wasmtime::component::__internal::Vec<u8>,
        )| {
            let host = get(caller.data_mut());
            host.put(key, value);
            Ok(())
        },
    )?;
    Ok(())
}

/// Registers both `@1.1.0` and `@1.0.0` secrets-store worlds on the provided linker.
pub fn add_secrets_store_compat_to_linker<T>(
    linker: &mut wasmtime::component::Linker<T>,
    get: fn(&mut T) -> &mut dyn SecretsStoreHostV1_1,
) -> anyhow::Result<()> {
    let mut inst_v1_1 = linker.instance("greentic:secrets-store/secrets-store@1.1.0")?;
    inst_v1_1.func_wrap(
        "get",
        move |mut caller: wasmtime::StoreContextMut<'_, T>,
              (key,): (wasmtime::component::__internal::String,)| {
            let host = get(caller.data_mut());
            let result = host.get(key);
            Ok((result,))
        },
    )?;
    inst_v1_1.func_wrap(
        "put",
        move |mut caller: wasmtime::StoreContextMut<'_, T>,
              (key, value): (
            wasmtime::component::__internal::String,
            wasmtime::component::__internal::Vec<u8>,
        )| {
            let host = get(caller.data_mut());
            host.put(key, value);
            Ok(())
        },
    )?;

    let mut inst_v1_0 = linker.instance("greentic:secrets-store/secrets-store@1.0.0")?;
    inst_v1_0.func_wrap(
        "get",
        move |mut caller: wasmtime::StoreContextMut<'_, T>,
              (key,): (wasmtime::component::__internal::String,)| {
            let host = get(caller.data_mut());
            let result = host.get(key);
            let mapped = match result {
                Ok(value) => Ok(value),
                Err(err) => Err(convert_error(err)),
            };
            Ok((mapped,))
        },
    )?;
    Ok(())
}

fn convert_error(err: bindings_v1_1::SecretsError) -> bindings_v1_0::SecretsError {
    match err {
        bindings_v1_1::SecretsError::NotFound => bindings_v1_0::SecretsError::NotFound,
        bindings_v1_1::SecretsError::Denied => bindings_v1_0::SecretsError::Denied,
        bindings_v1_1::SecretsError::InvalidKey => bindings_v1_0::SecretsError::InvalidKey,
        bindings_v1_1::SecretsError::Internal => bindings_v1_0::SecretsError::Internal,
    }
}
