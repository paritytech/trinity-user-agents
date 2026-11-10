use core::time::Duration;
use std::sync::atomic::{AtomicUsize, Ordering};

use futures::{
	FutureExt, StreamExt,
	future::{AbortHandle, Abortable},
	pin_mut,
};

use super::*;
use crate::{
	Spawner,
	chain_runtime::RuntimeChainProvider,
	test_support::{ScriptedProvider, extract_id, notification_sender, wait_for_sent},
};

const FOLLOW: &str = "FOLLOW";

/// A node whose follows are all `FOLLOW`, whose blocks share one runtime, and
/// which leaves the first `chainHead_v1_call` unanswered and starts the rest.
fn node() -> ScriptedProvider {
	let calls = AtomicUsize::new(0);
	ScriptedProvider::with_frames(move |request| {
		let id = extract_id(request).unwrap();
		let result = match serde_json::from_str::<Value>(request).unwrap()["method"].as_str() {
			Some("chainHead_v1_follow") => json!(FOLLOW),
			Some("state_getStorageHash") => json!(format!("0x{}", hex::encode([0xc1; 32]))),
			Some("chainHead_v1_call") if calls.fetch_add(1, Ordering::SeqCst) > 0 => {
				json!({"result": "started", "operationId": "download"})
			},
			_ => return Vec::new(),
		};
		vec![json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string()]
	})
}

fn metadata_call() -> Option<Box<RawValue>> {
	let block = format!("0x{}", hex::encode([0x11; 32]));
	raw(&json!([FOLLOW, block, "Metadata_metadata_at_version", "0x0f000000"])).ok()
}

fn within_five_seconds<T>(future: impl Future<Output = T>) -> Option<T> {
	futures::executor::block_on(async {
		let future = future.fuse();
		let deadline = futures_timer::Delay::new(Duration::from_secs(5)).fuse();
		pin_mut!(future, deadline);
		futures::select! {
			output = future => Some(output),
			() = deadline => None,
		}
	})
}

/// The node can end other operations on a follow while a download reply is
/// on its way. Those ends are kept only until no reply is left, counting a
/// reply whose caller went away, so a follow does not collect them for as
/// long as it lives.
#[test]
fn a_follow_keeps_no_operation_ends_once_no_download_reply_is_pending() {
	let provider = node();
	let connection = futures::executor::block_on(provider.connect(vec![0xab; 32])).unwrap();
	let client = RuntimeCallClient::new(
		HostRpcClient::new(connection, Spawner::for_test()),
		vec![0xab; 32],
		RuntimeCallCache::default(),
	);
	let follow_params = raw(&json!([true])).ok();
	let mut follow = futures::executor::block_on(client.subscribe_raw(
		"chainHead_v1_follow",
		follow_params,
		"chainHead_v1_unfollow",
	))
	.unwrap();
	let caller = client.clone();
	let (abort, registration) = AbortHandle::new_pair();
	let dropped_call = std::thread::spawn(move || {
		let call = caller.request_raw("chainHead_v1_call", metadata_call());
		futures::executor::block_on(Abortable::new(call, registration))
	});
	wait_for_sent(&provider, |sent| {
		sent.iter().any(|request| request.contains("chainHead_v1_call"))
	});
	let other_operation_done = json!({
		"jsonrpc": "2.0",
		"method": "chainHead_v1_followEvent",
		"params": {"subscription": FOLLOW, "result": {
			"event": "operationCallDone",
			"operationId": "other",
			"output": "0x00",
		}},
	});
	notification_sender(&provider)
		.unbounded_send(other_operation_done.to_string())
		.unwrap();
	within_five_seconds(follow.stream.next()).expect("the follow reports the other operation");
	abort.abort();
	for _ in 0..500 {
		if dropped_call.is_finished() {
			break;
		}
		std::thread::sleep(Duration::from_millis(10));
	}

	within_five_seconds(client.request_raw("chainHead_v1_call", metadata_call()))
		.expect("the node starts the download")
		.unwrap();

	assert_eq!(client.with_follow(FOLLOW, |state| state.early_ends.len()), Some(0));
}
