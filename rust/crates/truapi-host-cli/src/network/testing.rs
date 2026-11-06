//! Stack descriptors for tests, edited from the bundled paseo-next-v2 copy,
//! and loopback RPC nodes.

use trinity_stack::{Descriptor, Document};

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
