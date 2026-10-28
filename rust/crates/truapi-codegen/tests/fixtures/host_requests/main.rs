use std::{
	io::{BufRead, BufReader, Read, Write},
	process::{Child, ChildStdout, Command, Stdio},
	sync::{Arc, Mutex},
	time::Duration,
};

use core_runtime::{
	CallError,
	frame::{
		MESSAGE_TYPE_INTERRUPT, MESSAGE_TYPE_RECEIVE, MESSAGE_TYPE_START, MESSAGE_TYPE_STOP,
		ProtocolMessage,
	},
	subscription::HostInitiatedSubscriptionManager,
	transport::Transport,
};
use parity_scale_codec::{Decode, Encode};
use truapi::{
	caller::probe_answer,
	versioned::probe::{ProductProbeError, ProductProbeRequest, ProductProbeResponse},
};

struct ProductPeer {
	child: Child,
	output: BufReader<ChildStdout>,
}

impl ProductPeer {
	fn finish(mut self) {
		drop(self.child.stdin.take());
		let mut remaining = String::new();
		self.output.read_to_string(&mut remaining).unwrap();
		assert_eq!((self.child.wait().unwrap().success(), remaining), (true, String::new()));
	}
}

struct ProductProcess {
	subscriptions: Arc<HostInitiatedSubscriptionManager>,
	script: String,
	active: Mutex<Option<ProductPeer>>,
}

impl Transport for ProductProcess {
	fn send(&self, message: ProtocolMessage) {
		if message.payload.message_type == MESSAGE_TYPE_STOP {
			let mut peer = self.active.lock().unwrap().take().expect("a product awaiting Stop");
			writeln!(peer.child.stdin.as_mut().unwrap(), "{}", hex::encode(message.encode()))
				.unwrap();
			peer.finish();
			return;
		}
		assert_eq!(message.payload.message_type, MESSAGE_TYPE_START);
		assert!(self.active.lock().unwrap().is_none());
		let request = ProductProbeRequest::decode(&mut &message.payload.value[..]).unwrap();
		let success = matches!(request, ProductProbeRequest::V2(request) if request.outcome == 0);
		let mut child = Command::new("bun")
			.arg(&self.script)
			.arg(hex::encode(message.encode()))
			.stdin(Stdio::piped())
			.stdout(Stdio::piped())
			.spawn()
			.expect("start the TypeScript product");
		let output = BufReader::new(child.stdout.take().unwrap());
		let mut peer = ProductPeer { child, output };
		let mut line = String::new();
		peer.output.read_line(&mut line).unwrap();
		let bytes = hex::decode(line.trim()).unwrap();
		let mut input = &bytes[..];
		let response = ProtocolMessage::decode(&mut input).unwrap();
		assert_eq!(
			(
				&response.request_id,
				response.payload.trait_id,
				response.payload.method_id,
				response.payload.message_type,
				input,
			),
			(
				&message.request_id,
				message.payload.trait_id,
				message.payload.method_id,
				if success { MESSAGE_TYPE_RECEIVE } else { MESSAGE_TYPE_INTERRUPT },
				&[][..],
			)
		);
		if success {
			*self.active.lock().unwrap() = Some(peer);
		} else {
			peer.finish();
		}
		assert!(self.subscriptions.handle_message(response).is_none());
	}

	fn on_message(
		&self,
		_handler: Box<dyn Fn(ProtocolMessage) + Send + Sync>,
	) -> Box<dyn FnOnce()> {
		Box::new(|| {})
	}
}

fn main() {
	let (expire, expired) = futures::channel::oneshot::channel();
	std::thread::spawn(move || {
		std::thread::sleep(Duration::from_secs(15));
		let _ = expire.send(());
	});
	let exchange = async {
		let subscriptions = Arc::new(HostInitiatedSubscriptionManager::new());
		let transport: Arc<dyn Transport> = Arc::new(ProductProcess {
			subscriptions: subscriptions.clone(),
			script: std::env::args().nth(1).expect("TypeScript product path"),
			active: Mutex::new(None),
		});
		let mut outcomes = Vec::new();
		for outcome in 0..4 {
			let request = ProductProbeRequest::V2(truapi::v02::ProductProbeRequest {
				text: "probe".into(),
				outcome,
			});
			outcomes.push(probe_answer(&subscriptions, transport.clone(), request).await);
		}
		outcomes.push(
			probe_answer(
				&subscriptions,
				transport,
				ProductProbeRequest::V1(truapi::v01::ProductProbeRequest { text: "older".into() }),
			)
			.await,
		);
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
	};
	let result = futures::executor::block_on(futures::future::select(Box::pin(exchange), expired));
	assert!(matches!(result, futures::future::Either::Left(_)), "host exchange timed out");
	println!("host-request-roundtrip-ok");
}
