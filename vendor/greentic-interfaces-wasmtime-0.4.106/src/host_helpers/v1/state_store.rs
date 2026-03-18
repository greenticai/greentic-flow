use crate::state_store_store_v1_0::greentic::state::state_store as bindings;

/// Host trait for `greentic:state/store@1.0.0`.
pub use bindings::Host as StateStoreHost;
pub use bindings::{HostError as StateStoreError, OpAck, StateKey, TenantCtx};

/// Register the state-store world on the provided linker.
pub fn add_state_store_to_linker<T>(
    linker: &mut wasmtime::component::Linker<T>,
    get: fn(&mut T) -> &mut dyn StateStoreHost,
) -> anyhow::Result<()> {
    let mut instance = linker.instance("greentic:state/state-store@1.0.0")?;
    instance.func_wrap(
        "read",
        move |mut caller: wasmtime::StoreContextMut<'_, T>,
              (key, ctx): (bindings::StateKey, Option<bindings::TenantCtx>)| {
            let host = get(caller.data_mut());
            let result = host.read(key, ctx);
            Ok((result,))
        },
    )?;
    instance.func_wrap(
        "write",
        move |mut caller: wasmtime::StoreContextMut<'_, T>,
              (key, bytes, ctx): (
            bindings::StateKey,
            wasmtime::component::__internal::Vec<u8>,
            Option<bindings::TenantCtx>,
        )| {
            let host = get(caller.data_mut());
            let result = host.write(key, bytes, ctx);
            Ok((result,))
        },
    )?;
    instance.func_wrap(
        "delete",
        move |mut caller: wasmtime::StoreContextMut<'_, T>,
              (key, ctx): (bindings::StateKey, Option<bindings::TenantCtx>)| {
            let host = get(caller.data_mut());
            let result = host.delete(key, ctx);
            Ok((result,))
        },
    )?;
    Ok(())
}
