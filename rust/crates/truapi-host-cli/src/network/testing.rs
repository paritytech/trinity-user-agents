//! Stack descriptors and stacks for tests, edited from the bundled
//! paseo-next-v2 copy, and loopback RPC nodes.

use std::{
	path::Path,
	sync::{
		Arc,
		atomic::{AtomicUsize, Ordering},
	},
};

use sha2::{Digest, Sha256};
use trinity_stack::{Descriptor, Document, Pointer};
use truapi::latest::ChainIdentifier;

use super::{ChainEndpoint, Stack, stack::light_client};
use crate::chain::{ChainAccess, ChainBackend};

/// Paseo's light spec, answered from without network access.
pub const RELAY_LIGHT_SPEC: &str =
	include_str!("../../../truapi-provider/tests/fixtures/paseo.json");
/// The light spec of a People chain on [`RELAY_LIGHT_SPEC`]'s relay.
pub const PEOPLE_LIGHT_SPEC: &str =
	include_str!("../../../truapi-provider/tests/fixtures/paseo_people.json");

/// The bundle `built_in` ships, included here rather than read from
/// `built_in`, whose tests use this module.
const PASEO_NEXT_V2_BUNDLE: &[u8] = include_bytes!("../../stacks/paseo-next-v2.json");

/// The bundled paseo-next-v2 descriptor, edited, as plain JSON bytes.
pub fn edit_descriptor(edit: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
	let Ok(Document::Envelope { payload, .. }) = Document::parse(PASEO_NEXT_V2_BUNDLE) else {
		panic!("bundled paseo-next-v2 stack is not an envelope");
	};
	let mut descriptor: serde_json::Value =
		serde_json::from_slice(&payload).expect("invalid bundled payload");
	edit(&mut descriptor);
	serde_json::to_vec(&descriptor).expect("JSON value does not serialise")
}

/// The bundled paseo-next-v2 descriptor, edited.
pub fn edit_stack(edit: impl FnOnce(&mut serde_json::Value)) -> Descriptor {
	let plain = edit_descriptor(edit);
	Document::parse(&plain).expect("invalid test descriptor").descriptor().clone()
}

/// The bundled paseo-next-v2 descriptor with `role`'s chain edited.
pub fn edit_chain(role: &str, edit: impl FnOnce(&mut serde_json::Value)) -> Descriptor {
	edit_stack(|descriptor| {
		let chains = descriptor["chains"].as_array_mut().expect("chains is not an array");
		edit(chains.iter_mut().find(|chain| chain["role"] == role).expect("role missing"));
	})
}

/// The genesis hash a light spec fixture implies.
pub fn light_spec_genesis(spec: &str) -> [u8; 32] {
	trinity_stack::light_spec_genesis(spec.as_bytes()).expect("the fixture is a light spec")
}

/// `chain_backend` for a test stack. A light client would keep its specs in a
/// directory of the system's temporary one, never in the user's state.
pub fn test_access(chain_backend: ChainBackend) -> ChainAccess {
	match chain_backend {
		ChainBackend::Rpc => ChainAccess::Rpc,
		ChainBackend::LightClient => ChainAccess::LightClient {
			spec_directory: std::env::temp_dir().join("truapi-host-test-light-specs"),
		},
	}
}

/// The pointer test stacks are read under.
pub fn test_pointer() -> Pointer {
	Pointer::new("https://stack.example/descriptor.json", None).expect("a descriptor URL")
}

impl Stack {
	/// The descriptor this stack reads.
	pub fn descriptor(&self) -> &Descriptor {
		&self.descriptor
	}

	/// The endpoint of a served role; `None` for the relay chain, which no
	/// network routes.
	pub fn endpoint_for_role(&self, role: ChainIdentifier) -> Option<&ChainEndpoint> {
		match role {
			ChainIdentifier::People => Some(&self.chains.people),
			ChainIdentifier::Bulletin => Some(&self.chains.bulletin),
			ChainIdentifier::AssetHub => Some(&self.chains.asset_hub),
			ChainIdentifier::Relay => None,
		}
	}

	/// This stack with its descriptor edited as JSON.
	pub fn with_edit(&self, edit: impl FnOnce(&mut serde_json::Value)) -> Self {
		let mut document = serde_json::to_value(&self.descriptor).expect("descriptor not JSON");
		edit(&mut document);
		let descriptor = Descriptor::from_json(document).expect("invalid edited descriptor");
		Self::new(&test_pointer(), descriptor, &test_access(self.chain_backend()))
			.expect("edited stack refused")
	}

	/// This stack with the chain of `role` edited as JSON.
	pub fn with_chain_edit(&self, role: &str, edit: impl FnOnce(&mut serde_json::Value)) -> Self {
		self.with_edit(|descriptor| {
			let chains = descriptor["chains"].as_array_mut().expect("chains is not an array");
			edit(chains.iter_mut().find(|chain| chain["role"] == role).expect("role missing"));
		})
	}

	/// This stack on a light client that keeps its specs in `spec_directory`,
	/// with the relay and People replaced by the chains of
	/// [`RELAY_LIGHT_SPEC`] and [`PEOPLE_LIGHT_SPEC`], their light specs named
	/// at `relay_url` and `people_url`.
	pub fn with_light_specs_at(
		&self,
		relay_url: &str,
		people_url: &str,
		spec_directory: &Path,
	) -> Self {
		let edited =
			[("Relay", RELAY_LIGHT_SPEC, relay_url), ("People", PEOPLE_LIGHT_SPEC, people_url)]
				.into_iter()
				.fold(self.with_edit(|_| {}), |stack, (role, spec, url)| {
					let genesis = light_spec_genesis(spec);
					stack.with_chain_edit(role, |chain| {
						chain["genesisHash"] = format!("0x{}", hex::encode(genesis)).into();
						chain["lightSpec"] = serde_json::json!({
							"url": url,
							"sha256": format!("0x{}", hex::encode(Sha256::digest(spec.as_bytes()))),
						});
					})
				});
		let light = light_client(
			&test_pointer(),
			&edited.descriptor,
			&edited.chains,
			spec_directory.into(),
		)
		.expect("the edited stack names its light specs");
		Self { light_client: Some(Arc::new(light)), ..edited }
	}

	/// This stack with its `dub` service at `url`, so its identity backend is
	/// `url` with `/api/v1` appended.
	pub fn with_dub(&self, url: &str) -> Self {
		self.with_edit(|descriptor| {
			let services = descriptor["services"].as_array_mut().expect("services not an array");
			for service in services.iter_mut().filter(|service| service["kind"] == "dub") {
				service["urls"] = serde_json::json!([url]);
			}
		})
	}

	/// This stack with every chain's RPC list set to `url` alone.
	pub fn with_rpc(&self, url: &str) -> Self {
		self.with_edit(|descriptor| {
			for chain in descriptor["chains"].as_array_mut().expect("chains not an array") {
				chain["rpc"] = serde_json::json!([url]);
			}
		})
	}
}

/// A loopback WebSocket JSON-RPC node answering each request with
/// `answer(method, params)`.
pub async fn serve_rpc(
	answer: impl Fn(&str, &serde_json::Value) -> serde_json::Value + Clone + Send + 'static,
) -> String {
	use futures_util::{SinkExt, StreamExt};
	use tokio_tungstenite::tungstenite::Message;

	let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
		.await
		.expect("loopback bind failed");
	let url = format!("ws://{}", listener.local_addr().expect("no listener address"));
	tokio::spawn(async move {
		loop {
			let (socket, _) = listener.accept().await.expect("accept failed");
			let answer = answer.clone();
			tokio::spawn(async move {
				let mut socket =
					tokio_tungstenite::accept_async(socket).await.expect("handshake failed");
				while let Some(Ok(Message::Text(request))) = socket.next().await {
					let request: serde_json::Value =
						serde_json::from_str(&request).expect("request is not JSON");
					let method = request["method"].as_str().expect("request without a method");
					let result = answer(method, &request["params"]);
					let response = serde_json::json!({
						"jsonrpc": "2.0",
						"id": request["id"],
						"result": result,
					});
					if socket.send(Message::text(response.to_string())).await.is_err() {
						break;
					}
				}
			});
		}
	});
	url
}

/// A loopback node that accepts every connection and never answers.
pub async fn serve_silent() -> String {
	let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
		.await
		.expect("loopback bind failed");
	let url = format!("ws://{}", listener.local_addr().expect("no listener address"));
	tokio::spawn(async move {
		let mut sockets = Vec::new();
		while let Ok((socket, _)) = listener.accept().await {
			sockets.push(socket);
		}
	});
	url
}

/// A loopback HTTP server with [`RELAY_LIGHT_SPEC`] at `/relay.json` and
/// [`PEOPLE_LIGHT_SPEC`] at `/people.json`. Returns its base URL and the
/// number of requests it has answered.
pub async fn serve_light_specs() -> (String, Arc<AtomicUsize>) {
	use tokio::io::{AsyncReadExt, AsyncWriteExt};

	let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
		.await
		.expect("loopback bind failed");
	let url = format!("http://{}", listener.local_addr().expect("no listener address"));
	let requests = Arc::new(AtomicUsize::new(0));
	let answered = Arc::clone(&requests);
	tokio::spawn(async move {
		while let Ok((mut socket, _)) = listener.accept().await {
			let mut request = [0; 1024];
			let read = socket.read(&mut request).await.unwrap_or(0);
			let request = String::from_utf8_lossy(&request[..read]);
			let body = match request.split_whitespace().nth(1) {
				Some("/relay.json") => RELAY_LIGHT_SPEC,
				Some("/people.json") => PEOPLE_LIGHT_SPEC,
				_ => "",
			};
			let status = if body.is_empty() { "404 Not Found" } else { "200 OK" };
			let head = format!(
				"HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
				body.len()
			);
			answered.fetch_add(1, Ordering::Relaxed);
			let _ = socket.write_all(head.as_bytes()).await;
			let _ = socket.write_all(body.as_bytes()).await;
		}
	});
	(url, requests)
}
