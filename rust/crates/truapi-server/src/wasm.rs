//! wasm-bindgen surface. Exposes [`WasmProductRuntime`] to JavaScript hosts so
//! they can wire the TrUAPI core into a browser or worker shell.
//!
//! The browser side hands a `callbacks` object (a `JsBridge`) to the
//! constructor. The bridge implements every host-side capability the
//! [`truapi_platform::Platform`] trait set requires. Internally the bridge
//! is wrapped in a [`SendWrapper`] so it satisfies the `Send` bound the
//! platform trait set imposes; sound on wasm32 because the runtime is
//! single-threaded.

use core::cell::Cell;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use futures::channel::mpsc;
use futures::future::{AbortHandle, Abortable};
use futures::stream::{self, BoxStream, Stream, StreamExt};
use js_sys::{Array, Function, Reflect, Uint8Array};
use parity_scale_codec::{Decode, Encode};
use send_wrapper::SendWrapper;
use truapi::latest::HostPlatform;
use truapi::v01;
use truapi_platform::{
    ChainProvider, ChatPlatform, HopProvider, HostInfo, JsonRpcConnection, PairingHostConfig,
    PermissionStatusHost, PlatformInfo, PocketPlatform, ProductContext, ProductExecutionKind,
    RuntimeConfigValidationError,
};
#[cfg(feature = "wasm-signing-host")]
use truapi_platform::{CoinageWalletHost, IdentityBackendHost, SigningHostConfig};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

#[cfg(feature = "wasm-signing-host")]
use crate::SigningHostRuntime;
use crate::host_logic::worker::WorkerTransition;
use crate::subscription::Spawner;
use crate::{
    ChannelId, DebugEvent, DebugSink, FrameSink, PairingHostRuntime,
    PermissionAuthorizationRequest, PermissionAuthorizationStatus, ProductRuntime,
};

mod generated_bridge;

use generated_bridge::JsBridge;

/// Per-core JS channel: outgoing frames and teardown for one product core.
struct CoreChannel {
    emit_frame: Function,
    dispose: Function,
}

impl CoreChannel {
    fn from_js(callbacks: &JsValue) -> Result<Self, JsValue> {
        Ok(Self {
            emit_frame: get_function(callbacks, "emitFrame")?,
            dispose: get_optional_function(callbacks, "dispose")?.unwrap_or_else(noop_function),
        })
    }
}

struct WasmFrameSink {
    emit_frame: SendWrapper<Function>,
}

impl FrameSink for WasmFrameSink {
    fn emit_frame(&self, frame: Vec<u8>) {
        let frame = Uint8Array::from(frame.as_slice());
        if let Err(err) = self.emit_frame.call1(&JsValue::NULL, &frame) {
            web_sys::console::error_1(&err);
        }
    }
}

/// This core's wire-contract fingerprint, for a host to stamp on each debug
/// envelope it forwards to the debugger.
///
/// The frames a web host taps are encoded by *this* core, so the identity the
/// debugger checks has to come from here. A host that stamped its JS client's
/// hash instead would attest to a table it did not encode with: `dist/wasm/web/`
/// is a hand-built, gitignored artifact, so a stale core paired with a fresh
/// client would pass the identity check while emitting frames from a different
/// contract - exactly the silent mis-decode the fingerprint exists to stop.
#[wasm_bindgen(js_name = wireSchemaHash)]
pub fn wire_schema_hash() -> String {
    crate::generated::wire_table::TRUAPI_WIRE_SCHEMA_HASH.to_string()
}

/// Streams tapped debug frames out to a JS `debugEmit(channelId, dir, frame)`
/// callback so the host worker can forward them to the debugger it dials.
/// Dev-only: installed only when the host provides the callback, and
/// fire-and-forget - a failing callback is logged, never propagated.
struct WasmDebugSink {
    emit: SendWrapper<Function>,
}

impl DebugSink for WasmDebugSink {
    fn emit(&self, event: DebugEvent) {
        let DebugEvent::Frame {
            channel_id,
            dir,
            bytes,
        } = event;
        let frame = Uint8Array::from(bytes.as_slice());
        if let Err(err) = self.emit.call3(
            &JsValue::NULL,
            &JsValue::from_str(&channel_id.0),
            &JsValue::from_str(dir.wire_str()),
            &frame,
        ) {
            web_sys::console::error_1(&err);
        }
    }
}

struct WasmPlatform {
    bridge: SendWrapper<Arc<JsBridge>>,
}

impl WasmPlatform {
    fn new(bridge: Arc<JsBridge>) -> Self {
        Self {
            bridge: SendWrapper::new(bridge),
        }
    }
}

#[truapi_platform::async_trait]
impl ChainProvider for WasmPlatform {
    async fn connect(
        &self,
        genesis_hash: [u8; 32],
    ) -> Result<Box<dyn JsonRpcConnection>, v01::GenericError> {
        connect_js_rpc(self.bridge.chain_connect.clone(), genesis_hash, None).await
    }
}

#[truapi_platform::async_trait]
impl HopProvider for WasmPlatform {
    async fn allowed_hop_endpoints(
        &self,
        bulletin_genesis_hash: [u8; 32],
    ) -> Result<Vec<String>, v01::GenericError> {
        let encoded = invoke_bytes_return(
            &self.bridge.allowed_hop_endpoints,
            vec![Uint8Array::from(bulletin_genesis_hash.as_slice()).into()],
        )
        .await
        .map_err(generic)?;
        decode_bytes(encoded, "encoded HOP endpoint list did not decode").map_err(generic)
    }

    async fn connect_hop(
        &self,
        bulletin_genesis_hash: [u8; 32],
        endpoint: String,
    ) -> Result<Box<dyn JsonRpcConnection>, v01::GenericError> {
        let allowed = self.allowed_hop_endpoints(bulletin_genesis_hash).await?;
        truapi_platform::ensure_allowed_hop_endpoint(&endpoint, &allowed)?;
        connect_js_rpc(
            self.bridge.hop_connect.clone(),
            bulletin_genesis_hash,
            Some(endpoint),
        )
        .await
    }
}

/// Keep callback closures alive until an asynchronous host open settles, even
/// when its Rust caller is cancelled. A late connection is closed, not leaked.
fn connect_js_rpc(
    connect: Function,
    genesis_hash: [u8; 32],
    endpoint: Option<String>,
) -> impl Future<Output = Result<Box<dyn JsonRpcConnection>, v01::GenericError>> + Send {
    SendWrapper::new(async move {
        let (result_tx, result_rx) = futures::channel::oneshot::channel();
        wasm_bindgen_futures::spawn_local(async move {
            let result = open_js_rpc(connect, genesis_hash, endpoint).await;
            if let Err(Ok(connection)) = result_tx.send(result) {
                connection.close();
            }
        });
        result_rx
            .await
            .map_err(|_| generic("JSON-RPC connection open cancelled".into()))?
    })
}

async fn open_js_rpc(
    connect: Function,
    genesis_hash: [u8; 32],
    endpoint: Option<String>,
) -> Result<Box<dyn JsonRpcConnection>, v01::GenericError> {
    let private_rpc = endpoint.is_some();
    let (response_tx, response_rx) = mpsc::unbounded::<String>();
    let on_response_tx = response_tx.clone();
    let on_response = Closure::wrap(Box::new(move |json: JsValue| match json.as_string() {
        Some(json) => {
            let _ = on_response_tx.unbounded_send(json);
        }
        None => web_sys::console::error_1(&JsValue::from_str(
            "JSON-RPC onResponse expected a JSON string; dropping non-string value",
        )),
    }) as Box<dyn FnMut(JsValue)>);
    let on_closed_tx = response_tx.clone();
    let on_closed = Closure::wrap(Box::new(move || {
        on_closed_tx.close_channel();
    }) as Box<dyn FnMut()>);
    let mut args = vec![JsValue::from_str(&format!(
        "0x{}",
        hex::encode(genesis_hash)
    ))];
    if let Some(endpoint) = endpoint {
        args.push(JsValue::from_str(&endpoint));
    }
    args.push(on_response.as_ref().clone());
    args.push(on_closed.as_ref().clone());
    let returned = call_js_function(&connect, &args).map_err(generic)?;
    let resolved = await_optional_promise(returned).await.map_err(generic)?;
    if resolved.is_null() || resolved.is_undefined() {
        return Err(generic("JSON-RPC provider returned no connection".into()));
    }
    let close_fn = Reflect::get(&resolved, &JsValue::from_str("close"))
        .map_err(|_| generic("JSON-RPC connection must return { send, close }".into()))?
        .dyn_into::<Function>()
        .map_err(|_| generic("JSON-RPC connection.close must be a function".into()))?;
    let send_fn = Reflect::get(&resolved, &JsValue::from_str("send"))
        .ok()
        .and_then(|send| send.dyn_into::<Function>().ok());
    let Some(send_fn) = send_fn else {
        let _ = close_fn.call0(&JsValue::NULL);
        return Err(generic(
            "JSON-RPC connection.send must be a function".into(),
        ));
    };
    Ok(Box::new(JsCallbackJsonRpcConnection {
        send_fn: SendWrapper::new(send_fn),
        close_fn: SendWrapper::new(close_fn),
        closed: AtomicBool::new(false),
        private_rpc,
        _on_response: SendWrapper::new(on_response),
        _on_closed: SendWrapper::new(on_closed),
        response_tx,
        response_rx: std::sync::Mutex::new(Some(response_rx)),
    }))
}

// Account, signing, and statement-store flows live in the Rust core itself.
// The JS bridge only carries callbacks for platform capabilities the core
// cannot satisfy alone; account authority is selected by the runtime.

struct JsSubscriptionStream<T> {
    rx: mpsc::UnboundedReceiver<T>,
    _send_item: SendWrapper<Closure<dyn FnMut(JsValue)>>,
    _send_error: SendWrapper<Closure<dyn FnMut(JsValue)>>,
    dispose: Option<SendWrapper<Function>>,
}

impl<T> Stream for JsSubscriptionStream<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.rx).poll_next(cx)
    }
}

impl<T> Drop for JsSubscriptionStream<T> {
    fn drop(&mut self) {
        if let Some(dispose) = self.dispose.take() {
            let _ = dispose.call0(&JsValue::NULL);
        }
    }
}

fn invoke_js_subscription<T>(
    fn_: &Function,
    payload: Option<Vec<u8>>,
    parse_item: fn(JsValue) -> Result<T, String>,
) -> BoxStream<'static, Result<T, v01::GenericError>>
where
    T: Send + 'static,
{
    let (tx, rx) = mpsc::unbounded::<Result<T, v01::GenericError>>();
    let item_tx = tx.clone();
    let send_item = Closure::wrap(Box::new(move |value: JsValue| {
        let item = parse_item(value).map_err(generic);
        let _ = item_tx.unbounded_send(item);
    }) as Box<dyn FnMut(JsValue)>);
    let send_error = Closure::wrap(Box::new(move |value: JsValue| {
        let _ = tx.unbounded_send(Err(parse_generic_error(value)));
    }) as Box<dyn FnMut(JsValue)>);

    let call_result = match payload {
        Some(payload) => {
            let arg = Uint8Array::from(payload.as_slice());
            fn_.call3(
                &JsValue::NULL,
                &arg,
                send_item.as_ref().unchecked_ref(),
                send_error.as_ref().unchecked_ref(),
            )
        }
        None => fn_.call2(
            &JsValue::NULL,
            send_item.as_ref().unchecked_ref(),
            send_error.as_ref().unchecked_ref(),
        ),
    };

    let dispose = match call_result {
        Ok(value) if value.is_null() || value.is_undefined() => None,
        Ok(value) => match value.dyn_into::<Function>() {
            Ok(dispose) => Some(SendWrapper::new(dispose)),
            Err(_) => {
                return stream::once(async {
                    Err(generic(
                        "subscription callback must return a dispose function, null, or undefined"
                            .to_string(),
                    ))
                })
                .boxed();
            }
        },
        Err(err) => return stream::once(async { Err(generic(js_to_string(err))) }).boxed(),
    };

    Box::pin(JsSubscriptionStream {
        rx,
        _send_item: SendWrapper::new(send_item),
        _send_error: SendWrapper::new(send_error),
        dispose,
    })
}

struct JsCallbackJsonRpcConnection {
    send_fn: SendWrapper<Function>,
    close_fn: SendWrapper<Function>,
    closed: AtomicBool,
    private_rpc: bool,
    /// Closure must outlive the connection so JS keeps a live ref to the
    /// response sink. Dropped together with the rest of the struct.
    _on_response: SendWrapper<Closure<dyn FnMut(JsValue)>>,
    _on_closed: SendWrapper<Closure<dyn FnMut()>>,
    response_tx: mpsc::UnboundedSender<String>,
    response_rx: std::sync::Mutex<Option<mpsc::UnboundedReceiver<String>>>,
}

impl JsonRpcConnection for JsCallbackJsonRpcConnection {
    fn send(&self, request: String) {
        if self.closed.load(Ordering::Acquire) || self.response_tx.is_closed() {
            return;
        }
        let arg = JsValue::from_str(&request);
        if let Err(err) = self.send_fn.call1(&JsValue::NULL, &arg) {
            if self.private_rpc {
                web_sys::console::error_1(&JsValue::from_str("HOP send failed"));
            } else {
                web_sys::console::error_1(&err);
            }
            self.close();
        }
    }

    /// Single-take: the response receiver is handed out exactly once. A second
    /// call yields an empty stream (and logs), since the channel has one
    /// consumer.
    fn responses(&self) -> BoxStream<'static, String> {
        let mut guard = self.response_rx.lock().unwrap();
        match guard.take() {
            Some(rx) => rx.boxed(),
            None => {
                web_sys::console::error_1(&JsValue::from_str(
                    "JsCallbackJsonRpcConnection::responses() called more than once",
                ));
                futures::stream::empty().boxed()
            }
        }
    }

    fn close(&self) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        self.response_tx.close_channel();
        let _ = self.close_fn.call0(&JsValue::NULL);
    }
}

impl Drop for JsCallbackJsonRpcConnection {
    fn drop(&mut self) {
        self.close();
    }
}

fn generic(reason: String) -> v01::GenericError {
    v01::GenericError { reason }
}

fn parse_generic_error(value: JsValue) -> v01::GenericError {
    if let Some(reason) = value.as_string() {
        return generic(reason);
    }
    if let Ok(reason) = Reflect::get(&value, &JsValue::from_str("reason"))
        && let Some(reason) = reason.as_string()
    {
        return generic(reason);
    }
    generic(js_to_string(value))
}

/// Await the JS callback's return value if it's a Promise; pass other
/// values through unchanged. Every host callback resolves through this so
/// the JS side is free to be sync or async.
async fn await_optional_promise(returned: JsValue) -> Result<JsValue, String> {
    if returned.is_instance_of::<js_sys::Promise>() {
        let promise = returned.unchecked_into::<js_sys::Promise>();
        wasm_bindgen_futures::JsFuture::from(promise)
            .await
            .map_err(js_to_string)
    } else {
        Ok(returned)
    }
}

fn call_js_function(fn_: &Function, args: &[JsValue]) -> Result<JsValue, String> {
    let js_args = Array::new();
    for arg in args {
        js_args.push(arg);
    }
    fn_.apply(&JsValue::NULL, &js_args).map_err(js_to_string)
}

fn invoke_unit(
    fn_: &Function,
    args: Vec<JsValue>,
) -> impl Future<Output = Result<(), String>> + Send {
    let fn_ = fn_.clone();
    SendWrapper::new(async move {
        let returned = call_js_function(&fn_, &args)?;
        await_optional_promise(returned).await.map(|_| ())
    })
}

fn invoke_bool(
    fn_: &Function,
    args: Vec<JsValue>,
) -> impl Future<Output = Result<bool, String>> + Send {
    let fn_ = fn_.clone();
    SendWrapper::new(async move {
        let returned = call_js_function(&fn_, &args)?;
        let resolved = await_optional_promise(returned).await?;
        // A non-boolean resolved value is a host contract violation; surface it
        // rather than silently masking it as `false` (which would read as a
        // denial / unsupported and hide the host bug).
        resolved
            .as_bool()
            .ok_or_else(|| "callback must resolve to a boolean".to_string())
    })
}

fn invoke_bytes_return(
    fn_: &Function,
    args: Vec<JsValue>,
) -> impl Future<Output = Result<Vec<u8>, String>> + Send {
    let fn_ = fn_.clone();
    SendWrapper::new(async move {
        let returned = call_js_function(&fn_, &args)?;
        let resolved = await_optional_promise(returned).await?;
        resolved
            .dyn_into::<Uint8Array>()
            .map(|array| array.to_vec())
            .map_err(|_| "callback must resolve to Uint8Array".to_string())
    })
}

fn invoke_optional_bytes_return(
    fn_: &Function,
    args: Vec<JsValue>,
    expected: &'static str,
) -> impl Future<Output = Result<Option<Vec<u8>>, String>> + Send {
    let fn_ = fn_.clone();
    SendWrapper::new(async move {
        let returned = call_js_function(&fn_, &args)?;
        let resolved = await_optional_promise(returned).await?;
        if resolved.is_null() || resolved.is_undefined() {
            return Ok(None);
        }
        resolved
            .dyn_into::<Uint8Array>()
            .map(|array| Some(array.to_vec()))
            .map_err(|_| expected.to_string())
    })
}

fn invoke_optional_string_return(
    fn_: &Function,
    args: Vec<JsValue>,
) -> impl Future<Output = Result<Option<String>, String>> + Send {
    let fn_ = fn_.clone();
    SendWrapper::new(async move {
        let returned = call_js_function(&fn_, &args)?;
        let resolved = await_optional_promise(returned).await?;
        if resolved.is_null() || resolved.is_undefined() {
            return Ok(None);
        }
        resolved
            .as_string()
            .map(Some)
            .ok_or_else(|| "callback must resolve to string, null or undefined".to_string())
    })
}

fn decode_bytes<T: Decode>(bytes: Vec<u8>, message: &str) -> Result<T, String> {
    T::decode(&mut bytes.as_slice()).map_err(|_| message.to_string())
}

fn decode_js_item<T: Decode>(value: JsValue, label: &str) -> Result<T, String> {
    let bytes = value
        .dyn_into::<Uint8Array>()
        .map_err(|_| format!("{label} item must be Uint8Array"))?
        .to_vec();
    decode_bytes(bytes, &format!("encoded {label} item did not decode"))
}

fn parse_optional_bytes_item(value: JsValue) -> Result<Option<Vec<u8>>, String> {
    if value.is_null() || value.is_undefined() {
        return Ok(None);
    }
    value
        .dyn_into::<Uint8Array>()
        .map(|array| Some(array.to_vec()))
        .map_err(|_| "optional bytes item must be Uint8Array, null, or undefined".to_string())
}

fn js_to_string(value: JsValue) -> String {
    value
        .as_string()
        .or_else(|| {
            value
                .dyn_ref::<js_sys::Error>()
                .map(|err| err.message().into())
        })
        .unwrap_or_else(|| format!("{value:?}"))
}

fn get_function(callbacks: &JsValue, name: &str) -> Result<Function, JsValue> {
    let value = Reflect::get(callbacks, &JsValue::from_str(name))?;
    value
        .dyn_into::<Function>()
        .map_err(|_| JsValue::from_str(&format!("callbacks.{name} must be a function")))
}

fn get_optional_function(callbacks: &JsValue, name: &str) -> Result<Option<Function>, JsValue> {
    let value = Reflect::get(callbacks, &JsValue::from_str(name))?;
    if value.is_null() || value.is_undefined() {
        return Ok(None);
    }
    value
        .dyn_into::<Function>()
        .map(Some)
        .map_err(|_| JsValue::from_str(&format!("callbacks.{name} must be a function")))
}

/// Both stubs below are built from Rust closures rather than from source text:
/// `Function::new_no_args` compiles a string the way `eval` does, which a
/// Content-Security-Policy without `unsafe-eval` blocks even where it still
/// allows WebAssembly. They run at startup for every host, so a source-string
/// stub would keep the whole runtime from starting, not just the capability it
/// stands in for.
fn noop_function() -> Function {
    Closure::<dyn Fn()>::new(|| {})
        .into_js_value()
        .unchecked_into()
}

/// Stand-in for a callback of an optional capability the host left out. The
/// core only holds an adapter for a capability the bridge reports as present,
/// so this is never invoked; it throws rather than returning a value the
/// decoder would misread.
fn missing_callback(name: &str) -> Function {
    let message = format!("host callback {name} is not implemented");
    Closure::<dyn Fn() -> Result<(), JsValue>>::new(move || Err(JsValue::from_str(&message)))
        .into_js_value()
        .unchecked_into()
}

fn runtime_config_from_js(value: &JsValue) -> Result<(PairingHostConfig, ProductContext), JsValue> {
    let host_config = pairing_host_config_from_js(value)?;
    let product = product_context_from_js(value)?;
    Ok((host_config, product))
}

fn pairing_host_config_from_js(value: &JsValue) -> Result<PairingHostConfig, JsValue> {
    if value.is_null() || value.is_undefined() {
        return Err(JsValue::from_str("hostConfig is required"));
    }

    let host = get_required_object(value, "host", "runtimeConfig.host")?;
    let platform = get_optional_object(value, "platform", "runtimeConfig.platform")?;
    let people = get_required_object(value, "people", "runtimeConfig.people")?;
    let bulletin = get_required_object(value, "bulletin", "runtimeConfig.bulletin")?;
    let asset_hub = get_required_object(value, "assetHub", "runtimeConfig.assetHub")?;
    let pairing = get_required_object(value, "pairing", "runtimeConfig.pairing")?;

    PairingHostConfig::new(
        HostInfo {
            name: get_required_string_at(&host, "name", "runtimeConfig.host.name")?,
            icon: get_optional_string_at(&host, "icon", "runtimeConfig.host.icon")?,
            version: get_optional_string_at(&host, "version", "runtimeConfig.host.version")?,
            platform: host_platform_from_js(get_optional_string_at(
                &host,
                "platform",
                "runtimeConfig.host.platform",
            )?)?,
        },
        PlatformInfo {
            kind: platform
                .as_ref()
                .map(|p| get_optional_string_at(p, "type", "runtimeConfig.platform.type"))
                .transpose()?
                .flatten(),
            version: platform
                .as_ref()
                .map(|p| get_optional_string_at(p, "version", "runtimeConfig.platform.version"))
                .transpose()?
                .flatten(),
        },
        get_required_bytes32_at(&people, "genesisHash", "runtimeConfig.people.genesisHash")?,
        get_required_bytes32_at(
            &bulletin,
            "genesisHash",
            "runtimeConfig.bulletin.genesisHash",
        )?,
        get_required_bytes32_at(
            &asset_hub,
            "genesisHash",
            "runtimeConfig.assetHub.genesisHash",
        )?,
        get_required_string_at(
            &pairing,
            "deeplinkScheme",
            "runtimeConfig.pairing.deeplinkScheme",
        )?,
    )
    .map_err(runtime_config_validation_to_js)
}

/// Parse the optional `runtimeConfig.host.platform` category. Hosts that do
/// not declare one report `Unknown` to products.
fn host_platform_from_js(value: Option<String>) -> Result<HostPlatform, JsValue> {
    match value.as_deref() {
        None => Ok(HostPlatform::Unknown),
        Some("Web") => Ok(HostPlatform::Web),
        Some("Android") => Ok(HostPlatform::Android),
        Some("Ios") => Ok(HostPlatform::Ios),
        Some("Desktop") => Ok(HostPlatform::Desktop),
        Some("Cli") => Ok(HostPlatform::Cli),
        Some("Unknown") => Ok(HostPlatform::Unknown),
        Some(other) => Err(JsValue::from_str(&format!(
            "runtimeConfig.host.platform must be one of Web, Android, Ios, Desktop, Cli, or Unknown, got {other:?}"
        ))),
    }
}

#[cfg(feature = "wasm-signing-host")]
fn signing_host_config_from_js(value: &JsValue) -> Result<SigningHostConfig, JsValue> {
    if value.is_null() || value.is_undefined() {
        return Err(JsValue::from_str("hostConfig is required"));
    }

    let host = get_required_object(value, "host", "runtimeConfig.host")?;
    let platform = get_optional_object(value, "platform", "runtimeConfig.platform")?;
    let people = get_required_object(value, "people", "runtimeConfig.people")?;
    let bulletin = get_required_object(value, "bulletin", "runtimeConfig.bulletin")?;
    let asset_hub = get_required_object(value, "assetHub", "runtimeConfig.assetHub")?;
    let network_suffix =
        get_required_string_at(value, "networkSuffix", "runtimeConfig.networkSuffix")?;

    let mut config = SigningHostConfig::new(
        HostInfo {
            name: get_required_string_at(&host, "name", "runtimeConfig.host.name")?,
            icon: get_optional_string_at(&host, "icon", "runtimeConfig.host.icon")?,
            version: get_optional_string_at(&host, "version", "runtimeConfig.host.version")?,
            platform: host_platform_from_js(get_optional_string_at(
                &host,
                "platform",
                "runtimeConfig.host.platform",
            )?)?,
        },
        PlatformInfo {
            kind: platform
                .as_ref()
                .map(|p| get_optional_string_at(p, "type", "runtimeConfig.platform.type"))
                .transpose()?
                .flatten(),
            version: platform
                .as_ref()
                .map(|p| get_optional_string_at(p, "version", "runtimeConfig.platform.version"))
                .transpose()?
                .flatten(),
        },
        get_required_bytes32_at(&people, "genesisHash", "runtimeConfig.people.genesisHash")?,
        get_required_bytes32_at(
            &bulletin,
            "genesisHash",
            "runtimeConfig.bulletin.genesisHash",
        )?,
        get_required_bytes32_at(
            &asset_hub,
            "genesisHash",
            "runtimeConfig.assetHub.genesisHash",
        )?,
        network_suffix,
    )
    .map_err(runtime_config_validation_to_js)?;
    config.coinage_instance_id = get_optional_u32_at(
        value,
        "coinageInstanceId",
        "runtimeConfig.coinageInstanceId",
    )?;
    Ok(config)
}

fn product_context_from_js(value: &JsValue) -> Result<ProductContext, JsValue> {
    if value.is_null() || value.is_undefined() {
        return Err(JsValue::from_str("product is required"));
    }
    let product_id = get_required_string_at(value, "productId", "runtimeConfig.productId")?;
    let execution_kind =
        match get_optional_string_at(value, "executionKind", "runtimeConfig.executionKind")?
            .as_deref()
        {
            None | Some("App") => ProductExecutionKind::App,
            Some("Widget") => ProductExecutionKind::Widget,
            Some("Worker") => ProductExecutionKind::Worker,
            Some(other) => {
                return Err(JsValue::from_str(&format!(
                    "runtimeConfig.executionKind must be App, Widget or Worker, got {other:?}"
                )));
            }
        };
    ProductContext::new_with_execution(product_id, execution_kind)
        .map_err(runtime_config_validation_to_js)
}

fn runtime_config_field_to_js(field: &str) -> &str {
    match field {
        "product_id" => "productId",
        "host_info.name" => "host.name",
        "pairing_deeplink_scheme" => "pairing.deeplinkScheme",
        "people_chain_genesis_hash" => "people.genesisHash",
        "bulletin_chain_genesis_hash" => "bulletin.genesisHash",
        "asset_hub_chain_genesis_hash" => "assetHub.genesisHash",
        "network_suffix" => "networkSuffix",
        other => other,
    }
}

fn runtime_config_validation_to_js(err: RuntimeConfigValidationError) -> JsValue {
    match err {
        RuntimeConfigValidationError::EmptyField { field } => JsValue::from_str(&format!(
            "runtimeConfig.{} must not be empty",
            runtime_config_field_to_js(field)
        )),
        RuntimeConfigValidationError::InvalidHostIcon { source } => JsValue::from_str(&format!(
            "runtimeConfig.host.icon must be an absolute HTTPS URL: {source}"
        )),
        RuntimeConfigValidationError::InsecureHostIcon { scheme } => JsValue::from_str(&format!(
            "runtimeConfig.host.icon must use https scheme, got {scheme:?}"
        )),
        RuntimeConfigValidationError::InvalidDeeplinkScheme { scheme } => JsValue::from_str(
            &format!("runtimeConfig.pairing.deeplinkScheme must not include ://, got {scheme:?}"),
        ),
        RuntimeConfigValidationError::InvalidProductId { product_id } => {
            JsValue::from_str(&format!(
                "runtimeConfig.productId must be a dotNS or localhost product identifier, got {product_id:?}"
            ))
        }
        RuntimeConfigValidationError::InvalidNetworkSuffix { network_suffix } => {
            JsValue::from_str(&format!(
                "runtimeConfig.networkSuffix must be a supported dotNS TLD, got {network_suffix:?}"
            ))
        }
        // By length, never by value: an id that trips this is unbounded in
        // size, and this string reaches the product's console.
        RuntimeConfigValidationError::ProductIdTooLong { limit, actual } => JsValue::from_str(
            &format!("runtimeConfig.productId must be at most {limit} bytes, got {actual}"),
        ),
    }
}

fn get_required_object(value: &JsValue, name: &str, path: &str) -> Result<JsValue, JsValue> {
    let property = Reflect::get(value, &JsValue::from_str(name))?;
    if property.is_null() || property.is_undefined() {
        return Err(JsValue::from_str(&format!("{path} is required")));
    }
    if !property.is_object() {
        return Err(JsValue::from_str(&format!("{path} must be an object")));
    }
    Ok(property)
}

fn get_optional_object(
    value: &JsValue,
    name: &str,
    path: &str,
) -> Result<Option<JsValue>, JsValue> {
    let property = Reflect::get(value, &JsValue::from_str(name))?;
    if property.is_null() || property.is_undefined() {
        return Ok(None);
    }
    if !property.is_object() {
        return Err(JsValue::from_str(&format!("{path} must be an object")));
    }
    Ok(Some(property))
}

fn get_optional_string_at(
    value: &JsValue,
    name: &str,
    path: &str,
) -> Result<Option<String>, JsValue> {
    let property = Reflect::get(value, &JsValue::from_str(name))?;
    if property.is_null() || property.is_undefined() {
        return Ok(None);
    }
    property
        .as_string()
        .map(Some)
        .ok_or_else(|| JsValue::from_str(&format!("{path} must be a string")))
}

#[cfg(feature = "wasm-signing-host")]
fn get_optional_u32_at(value: &JsValue, name: &str, path: &str) -> Result<Option<u32>, JsValue> {
    let property = Reflect::get(value, &JsValue::from_str(name))?;
    if property.is_null() || property.is_undefined() {
        return Ok(None);
    }
    property
        .as_f64()
        .filter(|number| {
            number.is_finite() && number.fract() == 0.0 && (0.0..=u32::MAX as f64).contains(number)
        })
        .map(|number| Some(number as u32))
        .ok_or_else(|| JsValue::from_str(&format!("{path} must be an unsigned 32-bit integer")))
}

fn get_required_string_at(value: &JsValue, name: &str, path: &str) -> Result<String, JsValue> {
    get_optional_string_at(value, name, path)?
        .ok_or_else(|| JsValue::from_str(&format!("{path} is required")))
}

fn get_optional_bytes32_at(
    value: &JsValue,
    name: &str,
    path: &str,
) -> Result<Option<[u8; 32]>, JsValue> {
    let property = Reflect::get(value, &JsValue::from_str(name))?;
    if property.is_null() || property.is_undefined() {
        return Ok(None);
    }
    if let Some(hex) = property.as_string() {
        return parse_hex32(&hex)
            .map(Some)
            .map_err(|reason| JsValue::from_str(&format!("{path}: {reason}")));
    }
    let array = property
        .dyn_into::<Uint8Array>()
        .map_err(|_| JsValue::from_str(&format!("{path} must be hex or Uint8Array")))?;
    let bytes = array.to_vec();
    bytes.try_into().map(Some).map_err(|bytes: Vec<u8>| {
        JsValue::from_str(&format!(
            "{path} must be exactly 32 bytes, got {}",
            bytes.len()
        ))
    })
}

fn get_required_bytes32_at(value: &JsValue, name: &str, path: &str) -> Result<[u8; 32], JsValue> {
    get_optional_bytes32_at(value, name, path)?
        .ok_or_else(|| JsValue::from_str(&format!("{path} is required")))
}

fn parse_hex32(value: &str) -> Result<[u8; 32], String> {
    let raw = value.strip_prefix("0x").unwrap_or(value);
    if raw.len() != 64 {
        return Err(format!(
            "expected 32-byte hex string, got {} hex chars",
            raw.len()
        ));
    }
    let bytes = hex::decode(raw).map_err(|_| "invalid hex".to_string())?;
    bytes
        .try_into()
        .map_err(|bytes: Vec<u8>| format!("expected 32 bytes, got {}", bytes.len()))
}

fn decode_permission_authorization_request(
    payload: &[u8],
) -> Result<PermissionAuthorizationRequest, JsValue> {
    PermissionAuthorizationRequest::decode(&mut &*payload).map_err(|err| {
        JsValue::from_str(&format!(
            "permission authorization request did not decode: {err}"
        ))
    })
}

fn decode_permission_authorization_requests(
    payloads: &Array,
) -> Result<Vec<PermissionAuthorizationRequest>, JsValue> {
    let mut requests = Vec::with_capacity(payloads.length() as usize);
    for payload in payloads.iter() {
        let payload = payload
            .dyn_into::<Uint8Array>()
            .map_err(|_| JsValue::from_str("permission authorization request must be bytes"))?;
        requests.push(decode_permission_authorization_request(&payload.to_vec())?);
    }
    Ok(requests)
}

fn permission_authorization_status_to_js(status: PermissionAuthorizationStatus) -> JsValue {
    JsValue::from_str(match status {
        PermissionAuthorizationStatus::NotDetermined => "NotDetermined",
        PermissionAuthorizationStatus::Denied => "Denied",
        PermissionAuthorizationStatus::Authorized => "Authorized",
    })
}

fn permission_authorization_status_from_js(
    status: &str,
) -> Result<PermissionAuthorizationStatus, JsValue> {
    match status {
        "NotDetermined" => Ok(PermissionAuthorizationStatus::NotDetermined),
        "Denied" => Ok(PermissionAuthorizationStatus::Denied),
        "Authorized" => Ok(PermissionAuthorizationStatus::Authorized),
        other => Err(JsValue::from_str(&format!(
            "unknown permission authorization status: {other}"
        ))),
    }
}

fn generic_error_to_js(err: v01::GenericError) -> JsValue {
    JsValue::from_str(&err.reason)
}

struct WasmCoreInner {
    core: ProductRuntime,
    dispose_fn: SendWrapper<Function>,
    disposed: Cell<bool>,
    disposing: Cell<bool>,
}

struct WasmPlatformAdapters {
    platform: Arc<WasmPlatform>,
    chat_platform: Option<Arc<dyn ChatPlatform>>,
    status_host: Option<Arc<dyn PermissionStatusHost>>,
    pocket_platform: Option<Arc<dyn PocketPlatform>>,
    #[cfg(feature = "wasm-signing-host")]
    identity_backend_host: Option<Arc<dyn IdentityBackendHost>>,
    #[cfg(feature = "wasm-signing-host")]
    native_wallet: Option<Arc<dyn CoinageWalletHost>>,
}

/// Build the platform and the optional capability adapters supplied by the host.
fn wasm_platform(bridge: Arc<JsBridge>) -> WasmPlatformAdapters {
    let has_chat = bridge.has_chat();
    let has_permission_status = bridge.has_permission_status();
    let has_pocket = bridge.has_pocket();
    #[cfg(feature = "wasm-signing-host")]
    let has_identity_backend = bridge.has_identity_backend();
    #[cfg(feature = "wasm-signing-host")]
    let has_native_wallet = bridge.has_coinage_wallet();
    let platform = Arc::new(WasmPlatform::new(bridge));
    let chat = has_chat.then(|| platform.clone() as Arc<dyn ChatPlatform>);
    let status = has_permission_status.then(|| platform.clone() as Arc<dyn PermissionStatusHost>);
    let pocket = has_pocket.then(|| platform.clone() as Arc<dyn PocketPlatform>);
    #[cfg(feature = "wasm-signing-host")]
    let identity_backend =
        has_identity_backend.then(|| platform.clone() as Arc<dyn IdentityBackendHost>);
    #[cfg(feature = "wasm-signing-host")]
    let native_wallet = has_native_wallet.then(|| platform.clone() as Arc<dyn CoinageWalletHost>);
    WasmPlatformAdapters {
        platform,
        chat_platform: chat,
        status_host: status,
        pocket_platform: pocket,
        #[cfg(feature = "wasm-signing-host")]
        identity_backend_host: identity_backend,
        #[cfg(feature = "wasm-signing-host")]
        native_wallet,
    }
}

fn connection_adapters_from_js(
    callbacks: Option<&JsValue>,
) -> Result<Option<crate::host_core::ConnectionAdapters>, JsValue> {
    let Some(callbacks) = callbacks.filter(|value| !value.is_null() && !value.is_undefined())
    else {
        return Ok(None);
    };
    let WasmPlatformAdapters {
        platform,
        chat_platform,
        status_host,
        pocket_platform,
        ..
    } = wasm_platform(Arc::new(JsBridge::from_js(callbacks)?));
    Ok(Some(crate::host_core::ConnectionAdapters {
        platform,
        chat_platform,
        permission_status: status_host,
        pocket_platform,
        chat: Arc::new(crate::runtime::ActionChannel::chat()),
        renderer: Arc::new(crate::runtime::ActionChannel::renderer()),
    }))
}

/// Reports every worker demand transition to the host's
/// `workerDemandChanged(productId, transition)` callback.
struct WasmWorkerDemand {
    changed: SendWrapper<Function>,
}

impl crate::host_logic::worker::WorkerDemandObserver for WasmWorkerDemand {
    fn worker_demand_changed(&self, product_id: &str, transition: WorkerTransition) {
        let name = match transition {
            WorkerTransition::Start => "Start",
            WorkerTransition::Stop => "Stop",
        };
        let _ = self.changed.call2(
            &JsValue::NULL,
            &JsValue::from_str(product_id),
            &JsValue::from_str(name),
        );
    }
}

/// Install the host's `workerDemandChanged` callback as `ledger`'s observer.
/// Required: a host that omits it would never learn that a worker is wanted.
fn install_worker_demand_observer(
    ledger: &crate::host_logic::worker::WorkerLedger,
    callbacks: &JsValue,
) -> Result<(), JsValue> {
    let changed = get_function(callbacks, "workerDemandChanged")?;
    assert!(
        ledger.install_demand_observer(Arc::new(WasmWorkerDemand {
            changed: SendWrapper::new(changed),
        })),
        "a freshly built runtime installs its worker demand observer once"
    );
    Ok(())
}

/// JS-callable handle to a long-lived pairing-host runtime shared by product
/// cores.
#[wasm_bindgen]
pub struct WasmPairingHostRuntime {
    runtime: Rc<PairingHostRuntime>,
}

#[wasm_bindgen]
impl WasmPairingHostRuntime {
    /// Build a shared runtime from host-level platform callbacks and host config.
    #[wasm_bindgen(constructor)]
    pub fn new(
        callbacks: JsValue,
        host_config: JsValue,
    ) -> Result<WasmPairingHostRuntime, JsValue> {
        console_error_panic_hook::set_once();
        crate::logging::init();
        let bridge = Arc::new(JsBridge::from_js(&callbacks)?);
        let WasmPlatformAdapters {
            platform,
            chat_platform,
            status_host,
            pocket_platform,
            ..
        } = wasm_platform(bridge);
        let spawner: Spawner = Arc::new(|fut| {
            wasm_bindgen_futures::spawn_local(fut);
        });
        let host_config = pairing_host_config_from_js(&host_config)?;
        let runtime =
            PairingHostRuntime::with_chat_platform(platform, host_config, spawner, chat_platform);
        if let Some(status_host) = status_host {
            runtime.set_permission_status_host(status_host);
        }
        if let Some(pocket_platform) = pocket_platform {
            runtime.set_pocket_platform(pocket_platform);
        }
        install_worker_demand_observer(runtime.worker_ledger(), &callbacks)?;
        Ok(Self {
            runtime: Rc::new(runtime),
        })
    }

    /// Build one product-scoped runtime from this pairing host runtime.
    /// Optional platform callbacks are execution-local; shared authority stays here.
    #[wasm_bindgen(js_name = productRuntime)]
    pub fn product_runtime(
        &self,
        product: JsValue,
        core_callbacks: JsValue,
        platform_callbacks: Option<JsValue>,
    ) -> Result<WasmProductRuntime, JsValue> {
        let product = product_context_from_js(&product)?;
        let channel = CoreChannel::from_js(&core_callbacks)?;
        let debug_emit = get_optional_function(&core_callbacks, "debugEmit")?;
        let channel_id = product.product_id.clone();
        let sink = Arc::new(WasmFrameSink {
            emit_frame: SendWrapper::new(channel.emit_frame),
        });
        let runtime = match connection_adapters_from_js(platform_callbacks.as_ref())? {
            Some(adapters) => self.runtime.product_runtime_with(product, adapters, sink),
            None => self.runtime.product_runtime(product, sink),
        };
        if let Some(debug_emit) = debug_emit {
            runtime.set_debug_sink(
                ChannelId(channel_id),
                Arc::new(WasmDebugSink {
                    emit: SendWrapper::new(debug_emit),
                }),
            );
        }
        Ok(WasmProductRuntime::from_parts(runtime, channel.dispose))
    }

    /// Disconnect the shared account-authority session.
    #[wasm_bindgen(js_name = disconnectSession)]
    pub async fn disconnect_session(&self) {
        self.runtime.disconnect_session().await;
    }

    /// Cancel an in-flight pairing flow.
    #[wasm_bindgen(js_name = cancelPairing)]
    pub fn cancel_pairing(&self) {
        self.runtime.cancel_pairing();
    }

    /// Read the active session's X25519 chat identity private key, or
    /// `undefined` when no session is active.
    #[wasm_bindgen(js_name = sessionChatIdentityKey)]
    pub fn session_chat_identity_key(&self) -> Option<Vec<u8>> {
        self.runtime
            .session_chat_identity_key()
            .map(|key| key.to_vec())
    }

    /// Read this device's X25519 encryption secret, generating and persisting
    /// it on first read.
    #[wasm_bindgen(js_name = deviceEncryptionKey)]
    pub async fn device_encryption_key(&self) -> Result<Vec<u8>, JsValue> {
        self.runtime
            .device_encryption_key()
            .await
            .map(|key| key.to_vec())
            .map_err(generic_error_to_js)
    }

    /// Resolve a product's hard-subtree public key from the cache, the
    /// persisted slot, or the Account Holder. `timeoutMs` bounds that wait and
    /// exceeding it rejects. `undefined` when no session is active.
    #[wasm_bindgen(js_name = productSubtreePublicKey)]
    pub async fn product_subtree_public_key(
        &self,
        product_id: String,
        timeout_ms: Option<u32>,
    ) -> Result<Option<Vec<u8>>, JsValue> {
        self.runtime
            .product_subtree_public_key(&product_id, timeout_ms)
            .await
            .map(|key| key.map(|key| key.to_vec()))
            .map_err(generic_error_to_js)
    }

    /// Activate an externally persisted canonical session without writing it
    /// to core storage; resolves only after product frames may use it.
    #[wasm_bindgen(js_name = activateExternalSession)]
    pub async fn activate_external_session(&self, blob: Vec<u8>) -> Result<(), JsValue> {
        self.runtime
            .activate_external_session(&blob)
            .await
            .map_err(generic_error_to_js)
    }

    /// Restore the persisted auth session and resolve only after it is active.
    #[wasm_bindgen(js_name = activateStoredSession)]
    pub async fn activate_stored_session(&self) -> Result<(), JsValue> {
        self.runtime
            .activate_stored_session()
            .await
            .map_err(generic_error_to_js)
    }

    /// Notify the runtime that the auth session slot may have changed.
    #[wasm_bindgen(js_name = notifySessionStoreChanged)]
    pub fn notify_session_store_changed(&self) {
        self.runtime.notify_session_store_changed();
    }

    /// Read a permission authorization status for a product.
    ///
    /// A device capability resolves the host application's OS gate as well as
    /// storage, so an OS refusal reads as `Denied` whatever is stored. Remote,
    /// identity-disclosure and account-access decisions have no OS gate.
    #[wasm_bindgen(js_name = permissionAuthorizationStatus)]
    pub async fn permission_authorization_status(
        &self,
        product_id: String,
        payload: Vec<u8>,
    ) -> Result<JsValue, JsValue> {
        let request = decode_permission_authorization_request(&payload)?;
        let status = self
            .runtime
            .permission_authorization_status(&product_id, request)
            .await
            .map_err(generic_error_to_js)?;
        Ok(permission_authorization_status_to_js(status))
    }

    /// Read permission authorization statuses for a product.
    ///
    /// A device capability resolves the host application's OS gate as well as
    /// storage, so an OS refusal reads as `Denied` whatever is stored. Remote,
    /// identity-disclosure and account-access decisions have no OS gate.
    #[wasm_bindgen(js_name = permissionAuthorizationStatuses)]
    pub async fn permission_authorization_statuses(
        &self,
        product_id: String,
        payloads: Array,
    ) -> Result<Array, JsValue> {
        let requests = decode_permission_authorization_requests(&payloads)?;
        let statuses = self
            .runtime
            .permission_authorization_statuses(&product_id, requests)
            .await
            .map_err(generic_error_to_js)?;
        let values = Array::new();
        for status in statuses {
            values.push(&permission_authorization_status_to_js(status));
        }
        Ok(values)
    }

    /// Update a stored permission authorization status for a product.
    #[wasm_bindgen(js_name = setPermissionAuthorizationStatus)]
    pub async fn set_permission_authorization_status(
        &self,
        product_id: String,
        payload: Vec<u8>,
        status: String,
    ) -> Result<(), JsValue> {
        let request = decode_permission_authorization_request(&payload)?;
        let status = permission_authorization_status_from_js(&status)?;
        self.runtime
            .set_permission_authorization_status(&product_id, request, status)
            .await
            .map_err(generic_error_to_js)
    }

    /// Clear one product's durable and in-memory capability state.
    #[wasm_bindgen(js_name = clearProductState)]
    pub async fn clear_product_state(&self, product_id: String) -> Result<(), JsValue> {
        self.runtime
            .clear_product_state(&product_id)
            .await
            .map_err(generic_error_to_js)
    }

    /// Clear canonical paired-session state without notifying the peer.
    #[wasm_bindgen(js_name = resetSessionState)]
    pub async fn reset_session_state(&self) {
        self.runtime.reset_session_state().await;
    }

    /// Take one reference on the product's worker for a modality holder. The
    /// first one reports `"Start"` to the host's `workerDemandChanged`
    /// callback. Pair every call with one `releaseWorker`.
    #[wasm_bindgen(js_name = acquireWorker)]
    pub fn acquire_worker(&self, product_id: String) {
        self.runtime.worker_ledger().acquire(&product_id);
    }

    /// Release one reference. The last one reports `"Stop"`, after which the
    /// host may stop the worker; releasing with none held is a no-op.
    #[wasm_bindgen(js_name = releaseWorker)]
    pub fn release_worker(&self, product_id: String) {
        self.runtime.worker_ledger().release(&product_id);
    }
}

/// Whether `productId` is a first-party product the host grants every
/// `RemotePermission` without prompting.
///
/// Pure and stateless: it reads the compiled-in list and nothing else. **A
/// stored user decision wins over the list**, so this is only the answer for
/// the branch where the host's own store reads undetermined. Consulting it
/// first would let a revoked grant keep working.
///
/// `permissionAuthorizationStatus` is the stateful answer — it folds the list
/// and the stored decision together — and a host that can reach a runtime
/// should ask that instead.
///
/// This exists for the path where a host mediates product network access in its
/// own code — a service worker, a `fetch` shim — and has already found nothing
/// stored. Without it a first-party product is prompted by the host for access
/// the core would have granted.
///
/// Covers remote permissions only. Device capabilities, identity disclosure and
/// cross-product account access always prompt, whoever asks.
///
/// Normalizes before matching, and answers `false` for an id that does not
/// normalize, so an unknown spelling is never read as trusted.
#[wasm_bindgen(js_name = hasTrustedRemotePermissions)]
pub fn has_trusted_remote_permissions_for_wasm(product_id: String) -> bool {
    truapi_platform::normalizes_to_trusted_remote_permissions(&product_id)
}

/// Strictly decode a SCALE-encoded core-storage key for host storage policy.
#[wasm_bindgen(js_name = describeCoreStorageKey)]
pub fn describe_core_storage_key_for_wasm(encoded: Vec<u8>) -> Result<JsValue, JsValue> {
    let description = truapi_platform::describe_core_storage_key(&encoded)
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
    let value = js_sys::Object::new();
    Reflect::set(
        &value,
        &JsValue::from_str("kind"),
        &JsValue::from_str(description.kind),
    )?;
    if let Some(product_id) = description.product_id {
        Reflect::set(
            &value,
            &JsValue::from_str("productId"),
            &JsValue::from_str(&product_id),
        )?;
    }
    Ok(value.into())
}

#[cfg(feature = "wasm-signing-host")]
/// JS-callable handle to a wallet-local signing-host runtime.
#[wasm_bindgen]
pub struct WasmSigningHostRuntime {
    runtime: Rc<SigningHostRuntime>,
}

#[cfg(feature = "wasm-signing-host")]
#[wasm_bindgen]
impl WasmSigningHostRuntime {
    /// Build a shared signing runtime from host callbacks and host config.
    #[wasm_bindgen(constructor)]
    pub fn new(
        callbacks: JsValue,
        host_config: JsValue,
    ) -> Result<WasmSigningHostRuntime, JsValue> {
        console_error_panic_hook::set_once();
        crate::logging::init();
        let bridge = Arc::new(JsBridge::from_js(&callbacks)?);
        let WasmPlatformAdapters {
            platform,
            chat_platform,
            status_host,
            pocket_platform,
            identity_backend_host,
            native_wallet,
        } = wasm_platform(bridge);
        let spawner: Spawner = Arc::new(|fut| {
            wasm_bindgen_futures::spawn_local(fut);
        });
        let host_config = signing_host_config_from_js(&host_config)?;
        let runtime = SigningHostRuntime::with_chat_platform(
            platform,
            host_config,
            spawner,
            chat_platform,
            native_wallet,
        );
        if let Some(identity_backend_host) = identity_backend_host {
            runtime.set_identity_backend_host(identity_backend_host);
        }
        if let Some(status_host) = status_host {
            runtime.set_permission_status_host(status_host);
        }
        if let Some(pocket_platform) = pocket_platform {
            runtime.set_pocket_platform(pocket_platform);
        }
        install_worker_demand_observer(runtime.worker_ledger(), &callbacks)?;
        Ok(Self {
            runtime: Rc::new(runtime),
        })
    }

    /// Build one product-scoped runtime from this signing host.
    /// Optional platform callbacks are execution-local; custody stays on this host.
    #[wasm_bindgen(js_name = productRuntime)]
    pub fn product_runtime(
        &self,
        product: JsValue,
        core_callbacks: JsValue,
        platform_callbacks: Option<JsValue>,
    ) -> Result<WasmProductRuntime, JsValue> {
        let product = product_context_from_js(&product)?;
        let channel = CoreChannel::from_js(&core_callbacks)?;
        let sink = Arc::new(WasmFrameSink {
            emit_frame: SendWrapper::new(channel.emit_frame),
        });
        let runtime = match connection_adapters_from_js(platform_callbacks.as_ref())? {
            Some(adapters) => self.runtime.product_runtime_with(product, adapters, sink),
            None => self.runtime.product_runtime(product, sink),
        };
        Ok(WasmProductRuntime::from_parts(runtime, channel.dispose))
    }

    /// Disconnect the active wallet-local session.
    #[wasm_bindgen(js_name = disconnectSession)]
    pub async fn disconnect_session(&self) {
        self.runtime.disconnect_session().await;
    }

    /// Read the active local session's X25519 chat identity private key.
    #[wasm_bindgen(js_name = sessionChatIdentityKey)]
    pub fn session_chat_identity_key(&self) -> Option<Vec<u8>> {
        self.runtime
            .session_chat_identity_key()
            .map(|key| key.to_vec())
    }

    /// Read this browser's X25519 encryption secret, generating and persisting
    /// it on first read.
    #[wasm_bindgen(js_name = deviceEncryptionKey)]
    pub async fn device_encryption_key(&self) -> Result<Vec<u8>, JsValue> {
        self.runtime
            .device_encryption_key()
            .await
            .map(|key| key.to_vec())
            .map_err(generic_error_to_js)
    }

    /// Resolve a product's hard-subtree public key from the active local
    /// signing session.
    #[wasm_bindgen(js_name = productSubtreePublicKey)]
    pub async fn product_subtree_public_key(
        &self,
        product_id: String,
        timeout_ms: Option<u32>,
    ) -> Result<Option<Vec<u8>>, JsValue> {
        self.runtime
            .product_subtree_public_key(&product_id, timeout_ms)
            .await
            .map(|key| key.map(|key| key.to_vec()))
            .map_err(generic_error_to_js)
    }

    /// Read one permission authorization status for a product.
    #[wasm_bindgen(js_name = permissionAuthorizationStatus)]
    pub async fn permission_authorization_status(
        &self,
        product_id: String,
        payload: Vec<u8>,
    ) -> Result<JsValue, JsValue> {
        let request = decode_permission_authorization_request(&payload)?;
        let status = self
            .runtime
            .permission_authorization_status(&product_id, request)
            .await
            .map_err(generic_error_to_js)?;
        Ok(permission_authorization_status_to_js(status))
    }

    /// Read permission authorization statuses for a product.
    #[wasm_bindgen(js_name = permissionAuthorizationStatuses)]
    pub async fn permission_authorization_statuses(
        &self,
        product_id: String,
        payloads: Array,
    ) -> Result<Array, JsValue> {
        let requests = decode_permission_authorization_requests(&payloads)?;
        let statuses = self
            .runtime
            .permission_authorization_statuses(&product_id, requests)
            .await
            .map_err(generic_error_to_js)?;
        let values = Array::new();
        for status in statuses {
            values.push(&permission_authorization_status_to_js(status));
        }
        Ok(values)
    }

    /// Update one stored permission authorization status for a product.
    #[wasm_bindgen(js_name = setPermissionAuthorizationStatus)]
    pub async fn set_permission_authorization_status(
        &self,
        product_id: String,
        payload: Vec<u8>,
        status: String,
    ) -> Result<(), JsValue> {
        let request = decode_permission_authorization_request(&payload)?;
        let status = permission_authorization_status_from_js(&status)?;
        self.runtime
            .set_permission_authorization_status(&product_id, request, status)
            .await
            .map_err(generic_error_to_js)
    }

    /// Activate a wallet-local session from raw BIP-39 entropy.
    #[wasm_bindgen(js_name = activateLocalSession)]
    pub async fn activate_local_session(&self, secret: Vec<u8>) -> Result<(), JsValue> {
        self.runtime
            .activate_local_session(secret)
            .await
            .map_err(generic_error_to_js)
    }

    /// Activate a wallet-local session and attach known identity metadata.
    #[wasm_bindgen(js_name = activateLocalSessionWithIdentity)]
    pub async fn activate_local_session_with_identity(
        &self,
        secret: Vec<u8>,
        lite_username: Option<String>,
    ) -> Result<(), JsValue> {
        self.runtime
            .activate_local_session_with_identity(secret, lite_username)
            .await
            .map_err(generic_error_to_js)
    }

    /// Capture an opaque activation fence and its UID account.
    #[wasm_bindgen(js_name = localIdentityContext)]
    pub fn local_identity_context(&self) -> Result<JsValue, JsValue> {
        let context = self
            .runtime
            .local_identity_context()
            .map_err(generic_error_to_js)?;
        let json = serde_json::to_string(&context)
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        js_sys::JSON::parse(&json)
    }

    /// Return the UID public key followed by its native sr25519 backend-auth proof.
    #[wasm_bindgen(js_name = localIdentityAuthProof)]
    pub fn local_identity_auth_proof(
        &self,
        activation_id: String,
        challenge: Vec<u8>,
    ) -> Result<Vec<u8>, JsValue> {
        self.runtime
            .local_identity_auth_proof(&activation_id, &challenge)
            .map_err(generic_error_to_js)
    }

    /// Build registration JSON without exporting entropy or implementing proofs in JavaScript.
    #[wasm_bindgen(js_name = localLiteRegistrationBody)]
    pub async fn local_lite_registration_body(
        &self,
        activation_id: String,
        username_base: String,
        verifier: Vec<u8>,
    ) -> Result<String, JsValue> {
        let verifier = verifier
            .as_slice()
            .try_into()
            .map_err(|_| JsValue::from_str("attester must be 32 bytes"))?;
        self.runtime
            .local_lite_registration_body(&activation_id, &username_base, verifier)
            .await
            .map_err(generic_error_to_js)
    }

    /// Install freshly verified dotNS metadata only for the captured local activation.
    #[wasm_bindgen(js_name = refreshLocalIdentity)]
    pub async fn refresh_local_identity(&self, activation_id: String) -> Result<JsValue, JsValue> {
        let identity = self
            .runtime
            .refresh_local_identity_for(&activation_id)
            .await
            .map_err(generic_error_to_js)?;
        let json = serde_json::to_string(&identity)
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        js_sys::JSON::parse(&json)
    }

    /// Revoke one product's grants from the current local activation.
    #[wasm_bindgen(js_name = clearProductState)]
    pub async fn clear_product_state(&self, product_id: String) -> Result<(), JsValue> {
        self.runtime
            .clear_product_state(&product_id)
            .await
            .map_err(generic_error_to_js)
    }

    /// Take one reference on the product's worker for a modality holder. The
    /// first one reports `"Start"` to the host's `workerDemandChanged`
    /// callback. Pair every call with one `releaseWorker`.
    #[wasm_bindgen(js_name = acquireWorker)]
    pub fn acquire_worker(&self, product_id: String) {
        self.runtime.worker_ledger().acquire(&product_id);
    }

    /// Release one reference. The last one reports `"Stop"`, after which the
    /// host may stop the worker; releasing with none held is a no-op.
    #[wasm_bindgen(js_name = releaseWorker)]
    pub fn release_worker(&self, product_id: String) {
        self.runtime.worker_ledger().release(&product_id);
    }
}

/// Soft-derive a product account public key from a product's hard-subtree key
/// and a SCALE-encoded `DerivationIndex`.
///
/// The index crosses encoded rather than as a number so the chain code stays
/// core-owned: a host that rebuilds it wrongly gets a valid-looking wrong
/// address rather than an error.
#[wasm_bindgen(js_name = deriveProductAccountPublicKey)]
pub fn derive_product_account_public_key(
    product_subtree_public_key: Vec<u8>,
    derivation_index: Vec<u8>,
) -> Result<Vec<u8>, JsValue> {
    let subtree = <[u8; 32]>::try_from(product_subtree_public_key.as_slice())
        .map_err(|_| JsValue::from_str("product subtree public key must be 32 bytes"))?;
    let index = v01::DerivationIndex::decode(&mut derivation_index.as_slice())
        .map_err(|err| JsValue::from_str(&format!("derivation index did not decode: {err}")))?;
    crate::host_logic::product_account::derive_product_public_key(
        subtree,
        crate::host_logic::product_account::derivation_index_bytes(&index),
    )
    .map(|public_key| public_key.to_vec())
    .map_err(|err| JsValue::from_str(&err.to_string()))
}

/// Format a product account public key as the SS58 address host-spec C.6
/// mandates, so hosts do not each pick a prefix.
#[wasm_bindgen(js_name = productAccountAddress)]
pub fn product_account_address(public_key: Vec<u8>) -> Result<String, JsValue> {
    let public_key = <[u8; 32]>::try_from(public_key.as_slice())
        .map_err(|_| JsValue::from_str("product account public key must be 32 bytes"))?;
    Ok(crate::host_logic::product_account::product_public_key_to_address(public_key))
}

/// Set the live log level (`off`/`error`/`warn`/`info`/`debug`/`trace`).
/// Hosts may call this during boot, or again at any time to re-tune verbosity.
/// Unknown values are parsed as `off`.
#[wasm_bindgen(js_name = setLogLevel)]
pub fn set_log_level(level: &str) {
    crate::logging::set_level_from_str(level);
}

/// JS-callable handle to one product-scoped TrUAPI core.
#[wasm_bindgen]
pub struct WasmProductRuntime {
    inner: Rc<WasmCoreInner>,
}

impl WasmProductRuntime {
    fn from_parts(core: ProductRuntime, dispose_fn: Function) -> Self {
        Self {
            inner: Rc::new(WasmCoreInner {
                core,
                dispose_fn: SendWrapper::new(dispose_fn),
                disposed: Cell::new(false),
                disposing: Cell::new(false),
            }),
        }
    }
}

#[wasm_bindgen]
impl WasmProductRuntime {
    /// Build the core from a JS callbacks object. The object must define
    /// every host capability the [`truapi_platform::Platform`] trait set
    /// requires (camelCase property names; see the source for the full
    /// list).
    #[wasm_bindgen(constructor)]
    pub fn new(callbacks: JsValue, runtime_config: JsValue) -> Result<WasmProductRuntime, JsValue> {
        // Surface Rust panics to the browser console. A panic mid-dispatch
        // aborts the call as a wasm trap; the host should treat a thrown error
        // from `receiveFrame` as a fatal-instance signal and rebuild the
        // core rather than continue using it.
        console_error_panic_hook::set_once();
        crate::logging::init();
        let bridge = Arc::new(JsBridge::from_js(&callbacks)?);
        let channel = CoreChannel::from_js(&callbacks)?;
        let frame_sink = Arc::new(WasmFrameSink {
            emit_frame: SendWrapper::new(channel.emit_frame),
        });
        let WasmPlatformAdapters {
            platform,
            chat_platform,
            status_host,
            pocket_platform,
            ..
        } = wasm_platform(bridge);
        let spawner: Spawner = Arc::new(|fut| {
            wasm_bindgen_futures::spawn_local(fut);
        });
        let (host_config, product) = runtime_config_from_js(&runtime_config)?;
        // The optional adapters install on the host runtime the product hangs
        // off, not on the product runtime itself.
        let pairing =
            PairingHostRuntime::with_chat_platform(platform, host_config, spawner, chat_platform);
        if let Some(status_host) = status_host {
            pairing.set_permission_status_host(status_host);
        }
        if let Some(pocket_platform) = pocket_platform {
            pairing.set_pocket_platform(pocket_platform);
        }
        install_worker_demand_observer(pairing.worker_ledger(), &callbacks)?;
        let core = pairing.product_runtime(product, frame_sink);
        Ok(Self::from_parts(core, channel.dispose))
    }

    /// Push a SCALE-encoded protocol frame into the dispatcher. Responses
    /// (and subscription items) flow back through the `emitFrame`
    /// callback.
    #[wasm_bindgen(js_name = receiveFrame)]
    pub async fn receive_frame(&self, frame: Vec<u8>) -> Result<(), JsValue> {
        self.inner
            .core
            .receive_frame(frame)
            .await
            .map_err(|err| JsValue::from_str(&err.to_string()))
    }

    /// Read a permission authorization status without prompting.
    ///
    /// A device capability resolves the host application's OS gate as well as
    /// storage, so an OS refusal reads as `Denied` whatever is stored. Remote,
    /// identity-disclosure and account-access decisions have no OS gate.
    ///
    /// `payload` is a SCALE-encoded `PermissionAuthorizationRequest`.
    #[wasm_bindgen(js_name = permissionAuthorizationStatus)]
    pub async fn permission_authorization_status(
        &self,
        payload: Vec<u8>,
    ) -> Result<JsValue, JsValue> {
        let request = decode_permission_authorization_request(&payload)?;
        let status = self
            .inner
            .core
            .permission_authorization_status(request)
            .await
            .map_err(generic_error_to_js)?;
        Ok(permission_authorization_status_to_js(status))
    }

    /// Read permission authorization statuses without prompting.
    ///
    /// A device capability resolves the host application's OS gate as well as
    /// storage, so an OS refusal reads as `Denied` whatever is stored. Remote,
    /// identity-disclosure and account-access decisions have no OS gate.
    ///
    /// `payloads` is an array of SCALE-encoded
    /// `PermissionAuthorizationRequest` values. Results follow the same order.
    #[wasm_bindgen(js_name = permissionAuthorizationStatuses)]
    pub async fn permission_authorization_statuses(
        &self,
        payloads: Array,
    ) -> Result<Array, JsValue> {
        let requests = decode_permission_authorization_requests(&payloads)?;
        let statuses = self
            .inner
            .core
            .permission_authorization_statuses(requests)
            .await
            .map_err(generic_error_to_js)?;
        let values = Array::new();
        for status in statuses {
            values.push(&permission_authorization_status_to_js(status));
        }
        Ok(values)
    }

    /// Update a stored permission authorization status. Passing
    /// `"NotDetermined"` clears the stored value so the next product request
    /// prompts again.
    #[wasm_bindgen(js_name = setPermissionAuthorizationStatus)]
    pub async fn set_permission_authorization_status(
        &self,
        payload: Vec<u8>,
        status: String,
    ) -> Result<(), JsValue> {
        let request = decode_permission_authorization_request(&payload)?;
        let status = permission_authorization_status_from_js(&status)?;
        self.inner
            .core
            .set_permission_authorization_status(request, status)
            .await
            .map_err(generic_error_to_js)
    }

    /// Tear down the bridge. Invokes the JS-side `dispose` callback so the
    /// host can drop its end of the wiring.
    pub fn dispose(&self) -> Result<(), JsValue> {
        if self.inner.disposed.get() {
            return Ok(());
        }
        if self.inner.disposing.replace(true) {
            return Ok(());
        }

        self.inner.core.dispose();

        let result = self.inner.dispose_fn.call0(&JsValue::NULL).map(|_| ());

        self.inner.disposed.set(true);
        self.inner.disposing.set(false);
        result
    }

    /// Core-owned logout/disconnect. Best-effort notifies the SSO peer when
    /// the session has channel material, then clears in-memory and persisted
    /// session state.
    #[wasm_bindgen(js_name = disconnectSession)]
    pub async fn disconnect_session(&self) -> Result<(), JsValue> {
        self.inner.core.disconnect_session().await;
        Ok(())
    }

    /// Start the host-initiated render subscription for one body. `request` is
    /// a SCALE-encoded `ProductRendererRenderRequest`. `onUpdate` receives each
    /// replacement tree as a SCALE-encoded `RendererNode`. Exactly one terminal
    /// follows: `onComplete` when the stream ended with the last tree standing,
    /// or `onError` when the product could not serve the render and the last
    /// tree is partial. Rejects when this connection may not render.
    #[wasm_bindgen(js_name = render)]
    pub fn render(
        &self,
        request: Vec<u8>,
        on_update: Function,
        on_complete: Function,
        on_error: Function,
    ) -> Result<WasmRendererSubscription, JsValue> {
        let request = v01::ProductRendererRenderRequest::decode(&mut request.as_slice())
            .map_err(|err| JsValue::from_str(&format!("render request did not decode: {err}")))?;
        let mut stream = self
            .inner
            .core
            .control()
            .render(request)
            .map_err(|err| JsValue::from_str(&err.to_string()))?;
        let on_update = SendWrapper::new(on_update);
        let on_complete = SendWrapper::new(on_complete);
        let on_error = SendWrapper::new(on_error);
        let (abort, registration) = AbortHandle::new_pair();
        wasm_bindgen_futures::spawn_local(async move {
            let _ = Abortable::new(
                async move {
                    while let Some(item) = stream.next().await {
                        match item {
                            Ok(node) => {
                                let bytes = Uint8Array::from(node.encode().as_slice());
                                let _ = on_update.call1(&JsValue::NULL, &bytes);
                            }
                            Err(error) => {
                                let reason = crate::subscription::interrupt_reason(error);
                                let _ = on_error.call1(&JsValue::NULL, &JsValue::from_str(&reason));
                                return;
                            }
                        }
                    }
                    let _ = on_complete.call0(&JsValue::NULL);
                },
                registration,
            )
            .await;
        });
        Ok(WasmRendererSubscription { abort: Some(abort) })
    }

    /// Publish one host-authored Chat action into this connection's action
    /// stream, buffered until the product subscribes. Takes a SCALE-encoded
    /// `HostChatActionSubscribeItem`.
    #[wasm_bindgen(js_name = publishChatAction)]
    pub fn publish_chat_action(&self, action: Vec<u8>) -> Result<(), JsValue> {
        let action = v01::HostChatActionSubscribeItem::decode(&mut action.as_slice())
            .map_err(|err| JsValue::from_str(&format!("chat action did not decode: {err}")))?;
        self.inner
            .core
            .control()
            .publish_chat_action(action)
            .map_err(|err| JsValue::from_str(&err.to_string()))
    }

    /// Publish one action triggered inside a product-rendered body, buffered
    /// until the product subscribes. Takes a SCALE-encoded
    /// `HostRendererActionSubscribeItem`.
    #[wasm_bindgen(js_name = publishRendererAction)]
    pub fn publish_renderer_action(&self, item: Vec<u8>) -> Result<(), JsValue> {
        let item = v01::HostRendererActionSubscribeItem::decode(&mut item.as_slice())
            .map_err(|err| JsValue::from_str(&format!("renderer action did not decode: {err}")))?;
        self.inner
            .core
            .control()
            .publish_renderer_action(item)
            .map_err(|err| JsValue::from_str(&err.to_string()))
    }
}

/// Cancellable observation of one render instance. Dropping the handle on the
/// JS side does not stop the stream; call `cancel`.
#[wasm_bindgen]
pub struct WasmRendererSubscription {
    abort: Option<AbortHandle>,
}

#[wasm_bindgen]
impl WasmRendererSubscription {
    /// Stop delivering renderer updates. Idempotent.
    pub fn cancel(&mut self) {
        if let Some(abort) = self.abort.take() {
            abort.abort();
        }
    }
}
