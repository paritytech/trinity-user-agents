//! PROTOTYPE. Kettle's chat engine (`paritytech/kettle-core`) in the browser,
//! for the web host's Kettle Chat worker (`apps/kettle-chat/`).
//!
//! One [`KettleNode`] is one Kettle node: one account, which the worker
//! makes for one conversation. Its statement store and storage are
//! JavaScript ports ([`ports`]), so the worker signs through the core and
//! keeps the engine's sealed state in the product's storage. With a relay
//! from the worker, the node also runs Kettle's log transfer over iroh
//! ([`log_transfer`]). Kettle's own code runs unchanged.

mod events;
mod log_transfer;
mod ports;

use std::{rc::Rc, sync::Arc};

use js_sys::{Function, Object, Promise};
use kettle_chat::{ChatConfig, ChatNode, LogTransfer};
use kettle_crypto::sealed::Sealed;
use kettle_proto::{storage::Storage, store::StatementStore};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{future_to_promise, spawn_local};

use crate::{
	events::event_to_js,
	log_transfer::IrohStatus,
	ports::{JsPorts, error_text, fixed, hex_of, parse_hex, set},
};

/// The Kettle revision this module was built from.
#[wasm_bindgen(js_name = kettleRevision)]
pub fn kettle_revision() -> String {
	"4d0764d526e5e32dfc611323e5fa522651f864e6".to_string()
}

/// The iroh relays a node started with `options` may use (`options.iroh`):
/// none without it. Throws, without repeating the value, when it is refused.
#[wasm_bindgen(js_name = kettleIrohRelays)]
pub fn kettle_iroh_relays(options: JsValue) -> Result<js_sys::Array, JsValue> {
	log_transfer::relays_for(&options).map_err(js_error)
}

/// Whether a node started with `options` would dial a peer whose address
/// names `relays` (URLs): what its endpoint's hook decides before any packet
/// leaves. False when `options` asks for no iroh.
#[wasm_bindgen(js_name = kettleIrohAllows)]
pub fn kettle_iroh_allows(options: JsValue, relays: Vec<String>) -> Result<bool, JsValue> {
	log_transfer::allows(&options, &relays).map_err(js_error)
}

/// Kettle's protocol version, as its records and keys carry it.
#[wasm_bindgen(js_name = kettleProtocol)]
pub fn kettle_protocol() -> u8 {
	kettle_proto::wire::PROTOCOL_VERSION
}

/// A running Kettle node.
#[wasm_bindgen]
pub struct KettleNode {
	node: Rc<ChatNode>,
	/// The endpoint of the log transfer, closed when the node stops.
	endpoint: Option<iroh::Endpoint>,
	iroh: IrohStatus,
}

fn js_error(error: impl core::fmt::Display) -> JsValue {
	js_sys::Error::new(&error.to_string()).into()
}

fn bytes32(value: &str, what: &str) -> Result<[u8; 32], JsValue> {
	parse_hex(value).and_then(|bytes| fixed::<32>(&bytes, what)).map_err(js_error)
}

fn report_panics() {
	std::panic::set_hook(Box::new(|info| {
		let console = js_sys::Reflect::get(&js_sys::global(), &"console".into());
		if let Ok(error) =
			console.and_then(|console| js_sys::Reflect::get(&console, &"error".into())) &&
			let Ok(error) = error.dyn_into::<Function>()
		{
			error.call1(&JsValue::NULL, &format!("kettle: {info}").into()).ok();
		}
	}));
}

#[wasm_bindgen]
impl KettleNode {
	/// Start a node for `account` (`0x` hex) over `ports`, calling
	/// `onEvent` with each of its events. `options.fast` uses Kettle's short
	/// intervals, for tests that run in real time. `options.iroh`
	/// (`{ relays: "public" }` or `{ relays: "custom", relayUrl }`, with
	/// `onlineTimeoutMs?`) runs Kettle's log transfer on those relays, or
	/// without it when none is reached ([`Self::iroh`]).
	/// `options.onTrace` is called with each log transfer and each peer
	/// outbox ([`log_transfer::trace_to_js`]).
	pub async fn start(
		account: String,
		ports: JsValue,
		#[wasm_bindgen(js_name = onEvent)] on_event: Function,
		options: JsValue,
	) -> Result<KettleNode, JsValue> {
		report_panics();
		let account = bytes32(&account, "an account")?;
		let ports = JsPorts::new(account, ports);
		let store: Arc<dyn StatementStore> = Arc::new(ports.clone());
		let fast = js_sys::Reflect::get(&options, &"fast".into())
			.ok()
			.and_then(|value| value.as_bool())
			.unwrap_or(false);
		let config = if fast { ChatConfig::fast() } else { ChatConfig::default() };
		let (endpoint, iroh) = log_transfer::bind(&options).await;
		let mut builder = ChatNode::builder(store).config(config).storage(ports);
		if let Some(endpoint) = &endpoint {
			builder = builder.log_transfer(LogTransfer::iroh(endpoint.clone()));
		}
		let (node, mut events) = match builder.start().await {
			Ok(started) => started,
			Err(error) => {
				if let Some(endpoint) = &endpoint {
					endpoint.close().await;
				}
				return Err(js_error(error));
			},
		};
		if let Ok(on_trace) = js_sys::Reflect::get(&options, &"onTrace".into())
			.and_then(|value| value.dyn_into::<Function>())
		{
			let mut trace = node.trace();
			spawn_local(async move {
				use tokio::sync::broadcast::error::RecvError;
				loop {
					match trace.recv().await {
						Ok(event) => {
							let event = log_transfer::trace_to_js(&event);
							if !event.is_null() {
								on_trace.call1(&JsValue::NULL, &event).ok();
							}
						},
						Err(RecvError::Lagged(lost)) => {
							web_log(&format!("kettle: the trace lost {lost} events"))
						},
						Err(RecvError::Closed) => break,
					}
				}
			});
		}
		spawn_local(async move {
			while let Some(event) = events.recv().await {
				if let Err(error) = on_event.call1(&JsValue::NULL, &event_to_js(&event)) {
					web_log(&format!("kettle: an event listener threw: {}", error_text(&error)));
				}
			}
		});
		Ok(KettleNode { node: Rc::new(node), endpoint, iroh })
	}

	/// Remove the sealed state a node of `account` keeps in `ports`' storage,
	/// under the name and key the node derives from the account's entropy.
	/// Only while no node of `account` runs: one would write it again.
	#[wasm_bindgen(js_name = removeState)]
	pub async fn remove_state(account: String, ports: JsValue) -> Result<(), JsValue> {
		let ports = JsPorts::new(bytes32(&account, "an account")?, ports);
		let storage: Arc<dyn Storage> = Arc::new(ports.clone());
		let state = Sealed::for_store(&ports, storage).await.map_err(js_error)?;
		state.remove().await.map_err(js_error)
	}

	/// What the node runs of the log transfer: `{ state: "off" }`,
	/// `{ state: "on", relays, relay, endpoint }` or
	/// `{ state: "unavailable", relays?, relay?, reason }`.
	pub fn iroh(&self) -> JsValue {
		self.iroh.to_js()
	}

	/// The node's account, `0x` hex.
	pub fn account(&self) -> String {
		hex_of(&self.node.account())
	}

	/// Send text to `peer` in their direct chat, making the group when there
	/// is none. Answers `{ group, number, messageId, timeMs }` once the
	/// record is queued.
	#[wasm_bindgen(js_name = sendText)]
	pub fn send_text(&self, peer: String, text: String) -> Promise {
		let node = self.node.clone();
		future_to_promise(async move {
			let peer = bytes32(&peer, "a peer")?;
			let sent = node.send(peer, text).await.map_err(js_error)?;
			let object = Object::new();
			set(&object, "group", hex_of(&sent.group));
			set(&object, "number", sent.number as f64);
			if let Some(id) = sent.message_id {
				set(&object, "messageId", id);
			}
			set(&object, "timeMs", sent.time_ms as f64);
			Ok(object.into())
		})
	}

	/// The current direct group with `peer`, `0x` hex, or null.
	#[wasm_bindgen(js_name = directGroup)]
	pub fn direct_group(&self, peer: String) -> Promise {
		let node = self.node.clone();
		future_to_promise(async move {
			let peer = bytes32(&peer, "a peer")?;
			Ok(match node.direct_group(peer).await.map_err(js_error)? {
				Some(group) => hex_of(&group).into(),
				None => JsValue::NULL,
			})
		})
	}

	/// Leave `group` (`0x` hex).
	#[wasm_bindgen(js_name = leaveGroup)]
	pub fn leave_group(&self, group: String) -> Promise {
		let node = self.node.clone();
		future_to_promise(async move {
			let group = bytes32(&group, "a group")?;
			node.leave_group(group).await.map_err(js_error)?;
			Ok(JsValue::UNDEFINED)
		})
	}

	/// Write the sealed state now.
	#[wasm_bindgen(js_name = saveState)]
	pub fn save_state(&self) -> Promise {
		let node = self.node.clone();
		future_to_promise(async move {
			node.save_state().await.map_err(js_error)?;
			Ok(JsValue::UNDEFINED)
		})
	}

	/// What the node holds of `group`, as Kettle's inspection prints it, or null.
	pub fn inspect(&self, group: String) -> Promise {
		let node = self.node.clone();
		future_to_promise(async move {
			let group = bytes32(&group, "a group")?;
			Ok(match node.inspect(group).await.map_err(js_error)? {
				Some(inspection) => format!("{inspection:#?}").into(),
				None => JsValue::NULL,
			})
		})
	}

	/// Stop the node, then close its iroh endpoint. Its state stays in the storage.
	pub fn stop(&self) -> Promise {
		let node = self.node.clone();
		let endpoint = self.endpoint.clone();
		future_to_promise(async move {
			node.stop().await;
			if let Some(endpoint) = endpoint {
				endpoint.close().await;
			}
			Ok(JsValue::UNDEFINED)
		})
	}
}

fn web_log(line: &str) {
	if let Ok(console) = js_sys::Reflect::get(&js_sys::global(), &"console".into()) &&
		let Ok(log) = js_sys::Reflect::get(&console, &"warn".into()) &&
		let Ok(log) = log.dyn_into::<Function>()
	{
		log.call1(&JsValue::NULL, &line.into()).ok();
	}
}
