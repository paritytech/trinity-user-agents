use std::{process::Command, sync::Arc};

use core_runtime::{
	CallError, HostInitiatedRequestManager, frame::ProtocolMessage, transport::Transport,
};
use parity_scale_codec::{Decode, Encode};
use truapi::{
	caller::probe_answer,
	versioned::probe::{ProductProbeError, ProductProbeRequest, ProductProbeResponse},
};

struct ProductProcess {
	requests: Arc<HostInitiatedRequestManager>,
	script: String,
}

impl Transport for ProductProcess {
	fn send(&self, message: ProtocolMessage) {
		let output = Command::new("bun")
			.arg(&self.script)
			.arg(hex::encode(message.encode()))
			.output()
			.expect("start the TypeScript product");
		assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
		let bytes = hex::decode(String::from_utf8(output.stdout).unwrap().trim()).unwrap();
		let mut input = &bytes[..];
		let response = ProtocolMessage::decode(&mut input).unwrap();
		assert!(input.is_empty());
		assert!(self.requests.handle_message(response).is_none());
	}

	fn on_message(
		&self,
		_handler: Box<dyn Fn(ProtocolMessage) + Send + Sync>,
	) -> Box<dyn FnOnce()> {
		Box::new(|| {})
	}
}

fn main() {
	let requests = Arc::new(HostInitiatedRequestManager::new());
	let transport: Arc<dyn Transport> = Arc::new(ProductProcess {
		requests: requests.clone(),
		script: std::env::args().nth(1).expect("TypeScript product path"),
	});
	let mut outcomes = Vec::new();
	for outcome in 0..4 {
		let request = ProductProbeRequest::V2(truapi::v02::ProductProbeRequest {
			text: "probe".into(),
			outcome,
		});
		outcomes.push(futures::executor::block_on(probe_answer(
			&requests,
			transport.clone(),
			request,
		)));
	}
	outcomes.push(futures::executor::block_on(probe_answer(
		&requests,
		transport,
		ProductProbeRequest::V1(truapi::v01::ProductProbeRequest { text: "older".into() }),
	)));
	assert_eq!(
		outcomes,
		vec![
			Ok(ProductProbeResponse::V2(truapi::v01::ProductProbeResponse {
				text: "PROBE".into()
			})),
			Err(CallError::Domain(ProductProbeError::V2(
				truapi::v01::ProductProbeError::Rejected { reason: "probe".into() }
			))),
			Err(CallError::HostFailure { reason: "probe".into() }),
			Err(CallError::Denied),
			Err(CallError::Unsupported),
		]
	);
	println!("host-request-roundtrip-ok");
}
