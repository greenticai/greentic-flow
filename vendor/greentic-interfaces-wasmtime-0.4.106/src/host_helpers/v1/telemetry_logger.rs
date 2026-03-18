use crate::telemetry_logger_logger_v1_0::greentic::telemetry::logger_api as bindings;

/// Host trait for `greentic:telemetry/logger@1.0.0`.
pub use bindings::Host as TelemetryLoggerHost;
pub use bindings::{HostError as TelemetryLoggerError, OpAck, SpanContext, TenantCtx};

/// Register the telemetry-logger world on the provided linker.
pub fn add_telemetry_logger_to_linker<T>(
    linker: &mut wasmtime::component::Linker<T>,
    get: fn(&mut T) -> &mut dyn TelemetryLoggerHost,
) -> anyhow::Result<()> {
    let mut instance = linker.instance("greentic:telemetry/logger-api@1.0.0")?;
    instance.func_wrap(
        "log",
        move |mut caller: wasmtime::StoreContextMut<'_, T>,
              (span, fields, ctx): (
            bindings::SpanContext,
            wasmtime::component::__internal::Vec<(
                wasmtime::component::__internal::String,
                wasmtime::component::__internal::String,
            )>,
            Option<bindings::TenantCtx>,
        )| {
            let host = get(caller.data_mut());
            let result = host.log(span, fields, ctx);
            Ok((result,))
        },
    )?;
    Ok(())
}
