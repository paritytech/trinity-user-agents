//! Internal dispatcher/runtime core.
//!
//! Public host adapters should wrap this through [`crate::ProductRuntime`], which
//! owns the stable byte-frame ingress/egress and lifecycle API.

use std::sync::{Arc, Mutex};

use crate::platform::{PairingHostConfig, Platform, ProductContext};
use parity_scale_codec::{Decode, Encode};
use tracing::instrument;
use truapi::api::TrUApi;

use crate::{
	Spawner,
	dispatcher::Dispatcher,
	frame::ProtocolMessage,
	generated::dispatcher,
	host_logic::session::SessionState,
	runtime::{
		AccountHolder, HostAccounts, HostSession, PairingHostRole, ProductRuntimeHost,
		RuntimeServices,
	},
	transport::Transport,
};

/// Top-level core. Owns the generated dispatcher.
pub struct TrUApiCore {
	dispatcher: Dispatcher,
	session_state: Arc<SessionState>,
}

impl TrUApiCore {
	/// Build a core around a direct `TrUApi` implementation. The session
	/// state holder is unused on this path (no platform pushes updates),
	/// but is created anyway so the public API surface stays consistent.
	/// Subscription work runs on `spawner`.
	#[instrument(skip_all, fields(runtime.method = "core.new"))]
	pub fn new<P>(host: Arc<P>, spawner: Spawner) -> Self
	where
		P: TrUApi + 'static,
	{
		let mut dispatcher = Dispatcher::new(spawner);
		dispatcher::register(&mut dispatcher, host);
		Self { dispatcher, session_state: SessionState::new() }
	}

	/// Build a product-facing core around a [`Platform`] implementation,
	/// explicit host runtime config, and product context.
	#[instrument(skip_all, fields(runtime.method = "core.from_platform_with_config"))]
	pub fn from_platform_with_config<P>(
		platform: Arc<P>,
		host_config: PairingHostConfig,
		product: ProductContext,
		spawner: Spawner,
	) -> Self
	where
		P: Platform + 'static,
	{
		let platform: Arc<dyn Platform> = platform;
		let services = RuntimeServices::new(
			platform,
			host_config.host.host_info.clone(),
			host_config.people_chain_genesis_hash,
			host_config.bulletin_chain_genesis_hash,
			host_config.asset_hub_chain_genesis_hash,
			spawner.clone(),
		);
		let pairing_host = PairingHostRole::new(services.clone(), host_config);
		pairing_host.session().clone().start_session_store_sync(spawner);
		let session = pairing_host.session().clone();
		Self::from_runtime_parts(services, pairing_host.accounts().clone(), session, product)
	}

	/// Build a product-facing core from shared services and accounts.
	#[instrument(skip_all, fields(runtime.method = "core.from_runtime_parts"))]
	fn from_runtime_parts<H: AccountHolder>(
		services: Arc<RuntimeServices>,
		accounts: Arc<HostAccounts<H>>,
		host_session: Arc<dyn HostSession>,
		product: ProductContext,
	) -> Self {
		let session_state = host_session.session_state();
		let runtime = Arc::new(ProductRuntimeHost::from_services(
			services.clone(),
			crate::host_core::ConnectionAdapters::from_services(&services),
			accounts,
			host_session,
			product,
		));
		Self::from_product_runtime(runtime, services.spawner.clone(), session_state)
	}

	/// Build a dispatcher core around an already-created product runtime.
	#[instrument(skip_all, fields(runtime.method = "core.from_product_runtime"))]
	pub fn from_product_runtime<H: AccountHolder>(
		runtime: Arc<ProductRuntimeHost<H>>,
		spawner: Spawner,
		session_state: Arc<SessionState>,
	) -> Self {
		let execution_kind = runtime.connection().execution_kind();
		let mut dispatcher = Dispatcher::for_execution(spawner, execution_kind);
		dispatcher::register(&mut dispatcher, runtime);
		Self { dispatcher, session_state }
	}

	/// Handle to the shared session-state holder used by subscriptions and
	/// tests. Real host lifecycle flows through CoreStorage session sync and
	/// `disconnect`.
	pub fn session_state(&self) -> Arc<SessionState> {
		self.session_state.clone()
	}

	/// Decode an incoming product frame, run it through the dispatcher, and
	/// return the SCALE-encoded response frame when the method has one.
	/// Subscription starts should use [`Self::dispatch`] with a long-lived
	/// transport; changing this byte-frame helper to reject them or return a
	/// richer response shape is a separate API decision.
	#[instrument(skip_all, fields(runtime.method = "core.receive_from_product"))]
	pub async fn receive_from_product(&self, frame: &[u8]) -> Option<Vec<u8>> {
		let message = match ProtocolMessage::decode(&mut &*frame) {
			Ok(message) => message,
			Err(err) => {
				// An undecodable frame is a wire mismatch on the product's
				// side. Report it: dropping it unreported is indistinguishable
				// from the product never having sent it, and the product is
				// left waiting for a response that will never come.
				tracing::error!(
					frame_len = frame.len(),
					"undecodable product frame; dropping frame: {err}"
				);
				return None;
			},
		};
		let transport = Arc::new(ResponseTransport::default());
		self.dispatcher.dispatch(message, transport.clone() as Arc<dyn Transport>).await;
		transport.take().map(|response| response.encode())
	}

	/// Dispatch an already-decoded protocol message through the underlying
	/// dispatcher. Bridges that own a long-lived transport (e.g. WebSocket,
	/// JS callback) call this directly so subscription items flow back
	/// through the bridge transport instead of the single-slot capture used
	/// by [`Self::receive_from_product`].
	#[instrument(skip_all, fields(runtime.method = "core.dispatch"))]
	pub async fn dispatch(&self, message: ProtocolMessage, transport: Arc<dyn Transport>) {
		self.dispatcher.dispatch(message, transport).await;
	}

	/// Cancel all active and pending subscriptions owned by this core.
	pub fn cancel_subscriptions(&self) {
		self.dispatcher.cancel_subscriptions();
	}
}

/// Single-slot transport that captures the next response the dispatcher
/// emits. Used by [`TrUApiCore::receive_from_product`] to bridge between the
/// dispatcher's push model and the one-response frame API exposed to embedders.
#[derive(Default)]
struct ResponseTransport {
	response: Mutex<Option<ProtocolMessage>>,
}

impl ResponseTransport {
	fn take(&self) -> Option<ProtocolMessage> {
		self.response.lock().unwrap().take()
	}
}

impl Transport for ResponseTransport {
	fn send(&self, message: ProtocolMessage) {
		*self.response.lock().unwrap() = Some(message);
	}

	fn on_message(
		&self,
		_handler: Box<dyn Fn(ProtocolMessage) + Send + Sync>,
	) -> Box<dyn FnOnce()> {
		Box::new(|| {})
	}
}

#[cfg(test)]
mod tests;
