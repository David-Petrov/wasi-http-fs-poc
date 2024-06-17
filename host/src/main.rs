//! Example of instantiating a wasm module which uses WASI imports.

use std::sync::Arc;

// use wasi_common::sync::WasiCtxBuilder;
use wasmtime::*;

use anyhow::{anyhow, Context, Result};
use http_body::Frame;
use hyper::{body::{Buf, Bytes}, server::conn::http1, service::service_fn, Method, StatusCode};
use http_body_util::{combinators::BoxBody, Collected, Empty, StreamBody};
use wasmtime::{
    component::{Component, Linker, ResourceTable},
    Config, Engine, Store,
};
use wasmtime_wasi::{self, pipe::MemoryOutputPipe, DirPerms, FilePerms, WasiCtx, WasiCtxBuilder, WasiView};
use wasmtime_wasi_http::{
    bindings::http::types::ErrorCode,
    body::{HyperIncomingBody, HyperOutgoingBody},
    io::TokioIo,
    types::{self, HostFutureIncomingResponse, IncomingResponse, OutgoingRequestConfig},
    HttpResult, WasiHttpCtx, WasiHttpView,
};

type RequestSender = Arc<
    dyn Fn(hyper::Request<HyperOutgoingBody>, OutgoingRequestConfig) -> HostFutureIncomingResponse
        + Send
        + Sync,
>;

/// `Ctx` is our custom context type
struct Ctx {
    table: ResourceTable,
    wasi: WasiCtx,
    http: WasiHttpCtx,
    stdout: MemoryOutputPipe,
    stderr: MemoryOutputPipe,
    send_request: Option<RequestSender>,
    rejected_authority: Option<String>,
}

impl WasiView for Ctx {
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }
    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.wasi
    }
}

impl WasiHttpView for Ctx {
    fn ctx(&mut self) -> &mut WasiHttpCtx {
        &mut self.http
    }

    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }

    fn send_request(
        &mut self,
        request: hyper::Request<HyperOutgoingBody>,
        config: OutgoingRequestConfig,
    ) -> HttpResult<HostFutureIncomingResponse> {
        if let Some(rejected_authority) = &self.rejected_authority {
            let authority = request.uri().authority().map(ToString::to_string).unwrap();
            if &authority == rejected_authority {
                return Err(ErrorCode::HttpRequestDenied.into());
            }
        }
        if let Some(send_request) = self.send_request.clone() {
            Ok(send_request(request, config))
        } else {
            Ok(types::default_send_request(request, config))
        }
    }

    fn is_forbidden_header(&mut self, name: &hyper::header::HeaderName) -> bool {
        name.as_str() == "custom-forbidden-header"
    }
}

impl Drop for Ctx {
    fn drop(&mut self) {
        let stdout = self.stdout.contents();
        if !stdout.is_empty() {
            println!("[guest] stdout:\n{}\n===", String::from_utf8_lossy(&stdout));
        }
        let stderr = self.stderr.contents();
        if !stderr.is_empty() {
            println!("[guest] stderr:\n{}\n===", String::from_utf8_lossy(&stderr));
        }
    }
}

const PLUGIN_PATH: &str = "../target/wasm32-wasi/debug/plugin1.wasm";

#[tokio::main]
pub async fn main() -> anyhow::Result<()>  {
    use http_body_util::BodyExt;

    let req = hyper::Request::builder()
        .header("custom-forbidden-header", "yes")
        .uri("http://example.com:8080/test-path")
        .method(http::Method::GET);

    let empty_body = BoxBody::new(Empty::new().map_err(|_| unreachable!()));
    let resp = run_wasi_http(
        PLUGIN_PATH,
        req.body(empty_body)?,
        None,
        None,
    )
    .await?
    .map_err(|e| anyhow!("{e}"))?;

    println!("{resp:?}");

    Ok(())
}

async fn run_wasi_http(
    component_filename: &str,
    req: hyper::Request<HyperIncomingBody>,
    send_request: Option<RequestSender>,
    rejected_authority: Option<String>,
) -> anyhow::Result<Result<hyper::Response<Collected<Bytes>>, ErrorCode>> {
    let stdout = MemoryOutputPipe::new(4096);
    let stderr = MemoryOutputPipe::new(4096);
    let table = ResourceTable::new();

    let mut config = Config::new();
    config.wasm_backtrace_details(wasmtime::WasmBacktraceDetails::Enable);
    config.wasm_component_model(true);
    config.async_support(true);
    let engine = Engine::new(&config)?;
    let component = Component::from_file(&engine, component_filename)?;

    let wasi = WasiCtxBuilder::new()
        .stdout(stdout.clone())
        .stderr(stderr.clone())
        .preopened_dir(".", "/", DirPerms::READ, FilePerms::READ)?
        .build();

    let http = WasiHttpCtx::new();
    let ctx = Ctx {
        table,
        wasi,
        http,
        stderr,
        stdout,
        send_request,
        rejected_authority,
    };
    let mut store = Store::new(&engine, ctx);

    let mut linker = Linker::new(&engine);
    // Add proxy bindings (implementations) to the linker.
    wasmtime_wasi_http::proxy::add_to_linker(&mut linker)?;

    // Add filesystem bindings to the linker.
    let l = &mut linker;
    fn id<T>(x: &mut T) -> &mut T { x }
    wasmtime_wasi::bindings::filesystem::types::add_to_linker_get_host(l, id)?;
    wasmtime_wasi::bindings::filesystem::preopens::add_to_linker_get_host(l, id)?;

    // NOTE:
    //  All those bindings are due to me being unable to precisely identify which adapter I wish to use.... so I'm using the generic one with everything.
    //  This problem is with building the plugin component, not with the host. (I'm currently embedding more expectations than the plugin needs)
    wasmtime_wasi::bindings::cli::exit::add_to_linker_get_host(l, id)?;
    wasmtime_wasi::bindings::cli::environment::add_to_linker_get_host(l, id)?;

    wasmtime_wasi::bindings::cli::terminal_input::add_to_linker_get_host(l, id)?;
    wasmtime_wasi::bindings::cli::terminal_output::add_to_linker_get_host(l, id)?;
    wasmtime_wasi::bindings::cli::terminal_stdin::add_to_linker_get_host(l, id)?;
    wasmtime_wasi::bindings::cli::terminal_stdout::add_to_linker_get_host(l, id)?;
    wasmtime_wasi::bindings::cli::terminal_stderr::add_to_linker_get_host(l, id)?;

    let (proxy, _) =
        wasmtime_wasi_http::proxy::Proxy::instantiate_async(&mut store, &component, &linker)
            .await?;

    let req = store.data_mut().new_incoming_request(req)?;

    let (sender, receiver) = tokio::sync::oneshot::channel();
    let out = store.data_mut().new_response_outparam(sender)?;

    let handle = wasmtime_wasi::runtime::spawn(async move {
        proxy
            .wasi_http_incoming_handler()
            .call_handle(&mut store, req, out)
            .await?;

        Ok::<_, anyhow::Error>(())
    });

    let resp = match receiver.await {
        Ok(Ok(resp)) => {
            use http_body_util::BodyExt;
            let (parts, body) = resp.into_parts();
            let collected = BodyExt::collect(body).await?;
            Some(Ok(hyper::Response::from_parts(parts, collected)))
        }
        Ok(Err(e)) => Some(Err(e)),

        // Fall through below to the `resp.expect(...)` which will hopefully
        // return a more specific error from `handle.await`.
        Err(_) => None,
    };

    // Now that the response has been processed, we can wait on the wasm to
    // finish without deadlocking.
    handle.await.context("Component execution")?;

    Ok(resp.expect("wasm never called set-response-outparam"))
}
