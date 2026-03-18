use crate::http_client_client_v1_0::greentic::http::http_client as bindings_v1_0;
use crate::http_client_client_v1_1::greentic::http::http_client as bindings_v1_1;

/// Host trait for `greentic:http/client@1.0.0`.
pub use bindings_v1_0::Host as HttpClientHost;
pub use bindings_v1_0::{HostError as HttpClientError, Request, Response, TenantCtx};
pub type ImpersonationV1_0 =
    crate::http_client_client_v1_0::greentic::interfaces_types::types::Impersonation;

/// Host trait for `greentic:http/client@1.1.0`.
pub use bindings_v1_1::Host as HttpClientHostV1_1;
pub use bindings_v1_1::{
    HostError as HttpClientErrorV1_1, Request as RequestV1_1, RequestOptions as RequestOptionsV1_1,
    Response as ResponseV1_1, TenantCtx as TenantCtxV1_1,
};
pub type ImpersonationV1_1 =
    crate::http_client_client_v1_1::greentic::interfaces_types::types::Impersonation;

/// Register the HTTP client world on the provided linker.
pub fn add_http_client_to_linker<T>(
    linker: &mut wasmtime::component::Linker<T>,
    get: fn(&mut T) -> &mut dyn HttpClientHost,
) -> anyhow::Result<()> {
    let mut instance = linker.instance("greentic:http/http-client@1.0.0")?;
    instance.func_wrap(
        "send",
        move |mut caller: wasmtime::StoreContextMut<'_, T>,
              (req, ctx): (bindings_v1_0::Request, Option<bindings_v1_0::TenantCtx>)| {
            let host = get(caller.data_mut());
            let result = host.send(req, ctx);
            Ok((result,))
        },
    )?;
    Ok(())
}

/// Registers both `@1.1.0` and `@1.0.0` client worlds on the provided linker using a single host.
///
/// Calls to the legacy `@1.0.0` import are forwarded to the new host with `opts = None`.
pub fn add_http_client_compat_to_linker<T>(
    linker: &mut wasmtime::component::Linker<T>,
    get: fn(&mut T) -> &mut dyn HttpClientHostV1_1,
) -> anyhow::Result<()> {
    // New world exports the full surface.
    let mut inst_v1_1 = linker.instance("greentic:http/http-client@1.1.0")?;
    inst_v1_1.func_wrap(
        "send",
        move |mut caller: wasmtime::StoreContextMut<'_, T>,
              (req, opts, ctx): (
            bindings_v1_1::Request,
            Option<bindings_v1_1::RequestOptions>,
            Option<bindings_v1_1::TenantCtx>,
        )| {
            let host = get(caller.data_mut());
            let result = host.send(req, opts, ctx);
            Ok((result,))
        },
    )?;

    // Legacy world forwards into the new host by supplying None for request options.
    let mut inst_v1_0 = linker.instance("greentic:http/http-client@1.0.0")?;
    inst_v1_0.func_wrap(
        "send",
        move |mut caller: wasmtime::StoreContextMut<'_, T>,
              (req, ctx): (bindings_v1_0::Request, Option<bindings_v1_0::TenantCtx>)| {
            let host = get(caller.data_mut());
            let result = host.send(convert_request(req), None, ctx.map(convert_tenant_ctx));

            let mapped = match result {
                Ok(resp) => Ok(convert_response(resp)),
                Err(err) => Err(convert_error(err)),
            };
            Ok((mapped,))
        },
    )?;
    Ok(())
}

fn convert_request(req: bindings_v1_0::Request) -> bindings_v1_1::Request {
    bindings_v1_1::Request {
        method: req.method,
        url: req.url,
        headers: req.headers,
        body: req.body,
    }
}

fn convert_tenant_ctx(ctx: bindings_v1_0::TenantCtx) -> bindings_v1_1::TenantCtx {
    bindings_v1_1::TenantCtx {
        env: ctx.env,
        tenant: ctx.tenant,
        tenant_id: ctx.tenant_id,
        team: ctx.team,
        team_id: ctx.team_id,
        user: ctx.user,
        user_id: ctx.user_id,
        trace_id: ctx.trace_id,
        i18n_id: ctx.i18n_id,
        correlation_id: ctx.correlation_id,
        attributes: ctx.attributes,
        session_id: ctx.session_id,
        flow_id: ctx.flow_id,
        node_id: ctx.node_id,
        provider_id: ctx.provider_id,
        deadline_ms: ctx.deadline_ms,
        attempt: ctx.attempt,
        idempotency_key: ctx.idempotency_key,
        impersonation: ctx.impersonation.map(convert_impersonation),
    }
}

fn convert_impersonation(
    impersonation: crate::http_client_client_v1_0::greentic::interfaces_types::types::Impersonation,
) -> crate::http_client_client_v1_1::greentic::interfaces_types::types::Impersonation {
    crate::http_client_client_v1_1::greentic::interfaces_types::types::Impersonation {
        actor_id: impersonation.actor_id,
        reason: impersonation.reason,
    }
}

fn convert_response(resp: bindings_v1_1::Response) -> bindings_v1_0::Response {
    bindings_v1_0::Response {
        status: resp.status,
        headers: resp.headers,
        body: resp.body,
    }
}

fn convert_error(err: bindings_v1_1::HostError) -> bindings_v1_0::HostError {
    bindings_v1_0::HostError {
        code: err.code,
        message: err.message,
    }
}
