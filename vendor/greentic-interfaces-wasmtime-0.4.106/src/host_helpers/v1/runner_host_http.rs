use crate::host_runner_host_v1_0::greentic::host::http_v1 as bindings;

/// Host trait for `greentic:host/http-v1@1.0.0`.
pub use bindings::Host as RunnerHostHttp;

/// Register the runner-host HTTP world on the provided linker.
pub fn add_runner_host_http_to_linker<T>(
    linker: &mut wasmtime::component::Linker<T>,
    get: fn(&mut T) -> &mut dyn RunnerHostHttp,
) -> anyhow::Result<()> {
    let mut instance = linker.instance("greentic:host/http-v1@1.0.0")?;
    instance.func_wrap(
        "request",
        move |mut caller: wasmtime::StoreContextMut<'_, T>,
              (method, url, headers, body): (
            wasmtime::component::__internal::String,
            wasmtime::component::__internal::String,
            wasmtime::component::__internal::Vec<wasmtime::component::__internal::String>,
            Option<wasmtime::component::__internal::Vec<u8>>,
        )| {
            let host = get(caller.data_mut());
            let result = host.request(method, url, headers, body);
            Ok((result,))
        },
    )?;
    Ok(())
}
