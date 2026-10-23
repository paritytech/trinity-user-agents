//! Native WebSocket `ChainProvider`.
//!
//! The headless hosts reach the real People-chain statement store over
//! WebSocket JSON-RPC (the same node an iOS/web client uses). This module
//! picks the endpoint for a genesis hash, and every `connect` opens a fresh
//! socket through `truapi-provider`, which owns keepalive, shutdown, and
//! ending the response stream on disconnect. The runtime's `HostRpcClient`
//! sits on top, speaks statement-store RPC, and connects again once that
//! stream ends.

use std::collections::HashMap;

use async_trait::async_trait;
use tracing::debug;
use truapi::platform::{ChainProvider, JsonRpcConnection, ProviderError};

use crate::network::ChainEndpoint;

/// Chain provider that maps a requested genesis hash to a WebSocket endpoint.
///
/// The all-zero genesis (the headless SSO sentinel) and any unmapped genesis
/// fall back to the People-chain statement store. Every role the preset serves —
/// People, Bulletin and Asset Hub — is always routed; the test switch only widens
/// routing to endpoints the preset carries without serving them as a role.
pub struct WsChainProvider {
	fallback_url: String,
	by_genesis: HashMap<[u8; 32], String>,
}

impl WsChainProvider {
	/// Reads the test switch from the `E2E_LIVE_CHAIN` environment variable.
	pub fn new(fallback_url: impl Into<String>, live_chain_endpoints: &[ChainEndpoint]) -> Self {
		let live_chain_routing = std::env::var("E2E_LIVE_CHAIN").as_deref() == Ok("1");
		Self::with_live_chain_routing(fallback_url, live_chain_endpoints, live_chain_routing)
	}

	fn with_live_chain_routing(
		fallback_url: impl Into<String>,
		live_chain_endpoints: &[ChainEndpoint],
		live_chain_routing: bool,
	) -> Self {
		// People remains the fallback for the SSO sentinel. Bulletin backs preimage
		// submission and Asset Hub backs PGAS claims, so all three are host
		// dependencies and must never be gated by the product-facing Chain/* switch.
		let by_genesis = live_chain_endpoints
			.iter()
			.filter(|endpoint| endpoint.required_for_host || live_chain_routing)
			.map(|endpoint| (endpoint.genesis, endpoint.ws.to_string()))
			.collect();
		Self { fallback_url: fallback_url.into(), by_genesis }
	}

	/// Whether a genesis is mapped rather than answered by the fallback.
	///
	/// Test-only because production has no reason to care: `url_for` resolves either
	/// way. A test does, since the fallback is the People URL, so asserting on the
	/// resolved URL cannot tell a routed People from a dropped one.
	#[cfg(test)]
	fn routes(&self, genesis_hash: &[u8; 32]) -> bool {
		self.by_genesis.contains_key(genesis_hash)
	}

	/// The URL a genesis routes to. Test-only, so a routing override can be
	/// asserted rather than assumed: an override written to the wrong field
	/// compiles, changes nothing, and is invisible in timings.
	#[cfg(test)]
	pub fn routed_url(&self, genesis_hash: &[u8; 32]) -> &str {
		self.url_for(genesis_hash)
	}

	fn url_for(&self, genesis_hash: &[u8; 32]) -> &str {
		self.by_genesis
			.get(genesis_hash)
			.map(String::as_str)
			.unwrap_or(&self.fallback_url)
	}
}

#[async_trait]
impl ChainProvider for WsChainProvider {
	async fn connect(
		&self,
		genesis_hash: [u8; 32],
	) -> Result<Box<dyn JsonRpcConnection>, ProviderError> {
		let url = self.url_for(&genesis_hash);
		debug!(genesis = %hex::encode(genesis_hash), %url, "chain connect");
		// The URL is left out because this error can reach the product, and the URL
		// could carry credentials.
		let url = url.parse().map_err(|err| ProviderError::Host {
			reason: format!("chain endpoint is not a valid URL: {err}"),
		})?;
		truapi_provider::connect_rpc_node(url).await
	}
}

#[cfg(test)]
mod tests {
	use clap::ValueEnum;
	use futures_util::{SinkExt, StreamExt};
	use tokio_tungstenite::tungstenite::Message;

	use super::*;
	use crate::network::Network;

	/// The runtime keeps a chain connection open across calls and replaces it
	/// only once its response stream ends. A stream that outlived its socket
	/// would leave every later request on that chain waiting forever.
	#[tokio::test]
	async fn a_closed_socket_ends_the_stream_and_a_new_connection_replaces_it() {
		let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
			.await
			.expect("bind a loopback listener");
		let url = format!("ws://{}", listener.local_addr().expect("listener address"));
		tokio::spawn(async move {
			loop {
				let (socket, _) = listener.accept().await.expect("accept the client");
				let mut socket = tokio_tungstenite::accept_async(socket)
					.await
					.expect("complete the WebSocket handshake");
				socket.send(Message::text("open")).await.expect("send the first frame");
				socket.close(None).await.expect("close the socket");
			}
		});
		let provider = WsChainProvider::with_live_chain_routing(url, &[], false);

		let closed = provider.connect([0; 32]).await.expect("connect to the loopback server");
		let mut responses = closed.responses();
		assert_eq!(responses.next().await.as_deref(), Some("open"));
		let end = tokio::time::timeout(std::time::Duration::from_secs(5), responses.next())
			.await
			.expect("the response stream must end once the socket closes");
		assert_eq!(end, None);

		let replacement = provider.connect([0; 32]).await.expect("connect again");
		assert_eq!(replacement.responses().next().await.as_deref(), Some("open"));
	}

	/// Every role the host says it serves has to route to that role's own chain
	/// without the test switch. `url_for` answers an unmapped genesis with the
	/// fallback URL, so a served role that the routing filter drops would connect
	/// to the People chain while the host claimed to serve something else — and
	/// `ChainContextCache` only warns when the reported genesis diverges.
	#[test]
	fn every_served_role_routes_to_its_own_chain() {
		for network in Network::value_variants() {
			let config = network.config();
			let provider = WsChainProvider::with_live_chain_routing(
				config.people_ws,
				config.live_chain_endpoints,
				false,
			);
			for entry in config.host_chain_set().chains {
				let expected = config.url_for_role(entry.identifier).unwrap_or_else(|| {
					panic!("{} serves {:?} with no preset URL", config.id, entry.identifier)
				});
				assert!(
					provider.routes(&entry.genesis_hash),
					"{} serves {:?} but does not route it; the fallback would hide this",
					config.id,
					entry.identifier
				);
				assert_eq!(
					provider.url_for(&entry.genesis_hash),
					expected,
					"{} serves {:?} but routes it elsewhere",
					config.id,
					entry.identifier
				);
			}
		}
	}

	/// The switch exists to widen routing to endpoints the preset carries without
	/// serving them as a role. No preset has one now that Asset Hub is served, so
	/// this uses a synthetic endpoint: without a case the preset cannot express,
	/// `required_for_host` and the switch could both be deleted with a green suite.
	#[test]
	fn the_test_switch_widens_routing_to_endpoints_that_are_not_roles() {
		const FALLBACK: &str = "wss://fallback.invalid";
		let optional = [ChainEndpoint {
			genesis: [0x5a; 32],
			ws: "wss://optional.invalid",
			required_for_host: false,
		}];

		let gated = WsChainProvider::with_live_chain_routing(FALLBACK, &optional, false);
		let widened = WsChainProvider::with_live_chain_routing(FALLBACK, &optional, true);

		assert_eq!(
			gated.url_for(&optional[0].genesis),
			FALLBACK,
			"an endpoint that is not a role is excluded without the switch"
		);
		assert_eq!(widened.url_for(&optional[0].genesis), optional[0].ws, "and included with it");
	}
}
