use crate::host_runner_host_v1_0::greentic::host::kv_v1 as bindings;

/// Host trait for `greentic:host/kv-v1@1.0.0`.
pub use bindings::Host as RunnerHostKv;

/// Register the runner-host KV world on the provided linker.
pub fn add_runner_host_kv_to_linker<T>(
    linker: &mut wasmtime::component::Linker<T>,
    get: fn(&mut T) -> &mut dyn RunnerHostKv,
) -> anyhow::Result<()> {
    let mut instance = linker.instance("greentic:host/kv-v1@1.0.0")?;
    instance.func_wrap(
        "get",
        move |mut caller: wasmtime::StoreContextMut<'_, T>,
              (ns, key): (
            wasmtime::component::__internal::String,
            wasmtime::component::__internal::String,
        )| {
            let host = get(caller.data_mut());
            let result = host.get(ns, key);
            Ok((result,))
        },
    )?;
    instance.func_wrap(
        "put",
        move |mut caller: wasmtime::StoreContextMut<'_, T>,
              (ns, key, val): (
            wasmtime::component::__internal::String,
            wasmtime::component::__internal::String,
            wasmtime::component::__internal::String,
        )| {
            let host = get(caller.data_mut());
            host.put(ns, key, val);
            Ok(())
        },
    )?;
    Ok(())
}
