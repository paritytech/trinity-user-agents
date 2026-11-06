use super::*;
use crate::test_support::{ScriptedProvider, extract_id, notification_sender, wait_for_sent};
use futures::stream;
use parity_scale_codec::Encode;
use parking_lot::{Condvar, Mutex as ParkingMutex};
use std::sync::atomic::{AtomicUsize, Ordering};

fn spawner_for_tests() -> Spawner {
	#[cfg(not(target_arch = "wasm32"))]
	{
		crate::subscription::thread_per_subscription_spawner()
	}
	#[cfg(target_arch = "wasm32")]
	{
		Arc::new(futures::executor::block_on)
	}
}

/// A stop the node acknowledges.
const STOPPED: &str = r#""result":null"#;

/// A provider answering every broadcast with one provider operation id and
/// every stop with `stop_reply`, the JSON-RPC member that follows `"id"`.
fn broadcast_provider(stop_reply: &'static str) -> Arc<ScriptedProvider> {
	Arc::new(ScriptedProvider::new(move |request| {
		let id = extract_id(request).unwrap();
		if request.contains("transaction_v1_broadcast") {
			Some(format!(r#"{{"jsonrpc":"2.0","id":"{id}","result":"REMOTE-OP"}}"#))
		} else if request.contains("transaction_v1_stop") {
			Some(format!(r#"{{"jsonrpc":"2.0","id":"{id}",{stop_reply}}}"#))
		} else {
			None
		}
	}))
}

/// Broadcasts one transaction on `genesis` and returns the operation id handed out.
fn broadcast(runtime: &ChainRuntime, genesis: &[u8]) -> String {
	futures::executor::block_on(runtime.remote_chain_transaction_broadcast(
		RemoteChainTransactionBroadcastRequest {
			genesis_hash: genesis.to_vec(),
			transaction: vec![0],
		},
	))
	.expect("broadcast succeeds")
	.operation_id
	.expect("operation id")
}

fn stop(runtime: &ChainRuntime, genesis: &[u8], operation_id: &str) -> Result<(), RuntimeFailure> {
	futures::executor::block_on(runtime.remote_chain_transaction_stop(
		RemoteChainTransactionStopRequest {
			genesis_hash: genesis.to_vec(),
			operation_id: operation_id.to_owned(),
		},
	))
}

#[test]
fn chain_head_best_hash_prefers_best_block_after_initialization() {
	let mut follow = stream::iter(vec![
		RemoteChainHeadFollowItem::Initialized {
			finalized_block_hashes: vec![vec![0x01]],
			finalized_block_runtime: None,
		},
		RemoteChainHeadFollowItem::BestBlockChanged { best_block_hash: vec![0x02] },
	])
	.boxed();

	let hash = futures::executor::block_on(wait_for_chain_head_best_hash(
		&mut follow,
		"test chain",
		Duration::from_secs(10),
		Duration::from_secs(2),
	))
	.expect("best hash should resolve");

	assert_eq!(hash, vec![0x02]);
}

#[test]
fn chain_head_best_hash_timeout_falls_back_to_finalized_not_new_block() {
	let mut follow = stream::iter(vec![
		RemoteChainHeadFollowItem::Initialized {
			finalized_block_hashes: vec![vec![0x01]],
			finalized_block_runtime: None,
		},
		RemoteChainHeadFollowItem::NewBlock {
			block_hash: vec![0x03],
			parent_block_hash: vec![0x01],
			new_runtime: None,
		},
	])
	.chain(stream::pending())
	.boxed();

	let hash = futures::executor::block_on(wait_for_chain_head_best_hash(
		&mut follow,
		"test chain",
		Duration::from_secs(10),
		Duration::from_millis(1),
	))
	.expect("best hash should fall back to finalized hash");

	assert_eq!(hash, vec![0x01]);
}

#[test]
fn chain_head_best_hash_errors_on_stop_before_best_block() {
	let mut follow = stream::iter(vec![
		RemoteChainHeadFollowItem::Initialized {
			finalized_block_hashes: vec![vec![0x01]],
			finalized_block_runtime: None,
		},
		RemoteChainHeadFollowItem::NewBlock {
			block_hash: vec![0x03],
			parent_block_hash: vec![0x01],
			new_runtime: None,
		},
		RemoteChainHeadFollowItem::Stop,
	])
	.boxed();

	let err = futures::executor::block_on(wait_for_chain_head_best_hash(
		&mut follow,
		"test chain",
		Duration::from_secs(10),
		Duration::from_secs(2),
	))
	.expect_err("follow stop should be terminal before best block");

	assert_eq!(err, "test chain follow stopped before best block");
}

#[derive(Default)]
struct UnavailableChainProvider;

#[async_trait::async_trait]
impl RuntimeChainProvider for UnavailableChainProvider {
	async fn connect(
		&self,
		_genesis_hash: Vec<u8>,
	) -> Result<Arc<dyn JsonRpcConnection>, RuntimeFailure> {
		Err(RuntimeFailure::unavailable("remote_chain_connect"))
	}
}

#[test]
fn unavailable_provider_surfaces_failure() {
	let provider = Arc::new(UnavailableChainProvider);
	let result = futures::executor::block_on(provider.connect(vec![0u8; 32]));
	let err = match result {
		Ok(_) => panic!("expected failure"),
		Err(err) => err,
	};
	assert_eq!(err.kind(), RuntimeFailureKind::Unavailable);
	assert_eq!(err.method(), "remote_chain_connect");
}

#[test]
fn header_request_reuses_existing_follow() {
	let provider = Arc::new(ScriptedProvider::new(|request| {
		let id = extract_id(request).unwrap();
		if request.contains("chainHead_v1_follow") {
			Some(format!(r#"{{"jsonrpc":"2.0","id":"{id}","result":"REMOTE-FOLLOW"}}"#))
		} else if request.contains("chainHead_v1_header") {
			Some(format!(r#"{{"jsonrpc":"2.0","id":"{id}","result":"0xdeadbeef"}}"#))
		} else {
			None
		}
	}));
	let runtime = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let _follow_stream = runtime.remote_chain_head_follow(
		"local-follow".to_string(),
		RemoteChainHeadFollowRequest { genesis_hash: vec![0u8; 32], with_runtime: false },
	);
	let sent = wait_for_sent(&provider, |sent| {
		sent.iter().any(|request| request.contains("chainHead_v1_follow"))
	});
	assert!(
		sent.iter().any(|request| request.contains("chainHead_v1_follow")),
		"follow setup did not start; sent: {sent:?}",
	);

	let response = futures::executor::block_on(runtime.remote_chain_head_header(
		RemoteChainHeadHeaderRequest {
			genesis_hash: vec![0u8; 32],
			follow_subscription_id: "local-follow".to_string(),
			hash: vec![1u8; 32],
		},
	))
	.expect("ok response");
	assert_eq!(response.header, Some(vec![0xde, 0xad, 0xbe, 0xef]));
	assert_eq!(provider.connect_calls.load(Ordering::SeqCst), 1);
	let sent = provider.sent.lock().unwrap().clone();
	assert_eq!(sent.len(), 2);
	assert!(sent[0].contains("chainHead_v1_follow"));
	assert!(sent[1].contains("chainHead_v1_header"));
}

#[test]
fn header_waits_for_a_just_created_follow() {
	let provider = Arc::new(ScriptedProvider::new(|request| {
		let id = extract_id(request).unwrap();
		if request.contains("chainHead_v1_follow") {
			Some(format!(r#"{{"jsonrpc":"2.0","id":"{id}","result":"REMOTE-FOLLOW"}}"#))
		} else if request.contains("chainHead_v1_header") {
			let request: Value = serde_json::from_str(request).unwrap();
			assert_eq!(request["params"][0], "REMOTE-FOLLOW");
			Some(format!(r#"{{"jsonrpc":"2.0","id":"{id}","result":"0xdeadbeef"}}"#))
		} else {
			None
		}
	}));
	let gate = Arc::new((ParkingMutex::new(false), Condvar::new()));
	let spawner_gate = gate.clone();
	let spawner: Spawner = Arc::new(move |future| {
		let task_gate = spawner_gate.clone();
		std::thread::spawn(move || {
			let (released, ready) = &*task_gate;
			let mut released = released.lock();
			ready.wait_while(&mut released, |released| !*released);
			drop(released);
			futures::executor::block_on(future);
		});
	});
	let runtime = ChainRuntime::new(provider.clone(), spawner);
	let _follow_stream = runtime.remote_chain_head_follow(
		"c1:follow-ab".to_string(),
		RemoteChainHeadFollowRequest { genesis_hash: vec![0u8; 32], with_runtime: false },
	);

	let (result_sender, result_receiver) = std::sync::mpsc::channel();
	let header_runtime = runtime.clone();
	std::thread::spawn(move || {
		let result = futures::executor::block_on(header_runtime.remote_chain_head_header(
			RemoteChainHeadHeaderRequest {
				genesis_hash: vec![0u8; 32],
				follow_subscription_id: "c1:follow_0".to_string(),
				hash: vec![1u8; 32],
			},
		));
		result_sender.send(result).unwrap();
	});
	for _ in 0..100 {
		if provider.connect_calls.load(Ordering::SeqCst) == 1 {
			break;
		}
		std::thread::sleep(std::time::Duration::from_millis(1));
	}
	assert_eq!(
		provider.connect_calls.load(Ordering::SeqCst),
		1,
		"the follow-bound request opened the shared connection",
	);

	let early = result_receiver.recv_timeout(std::time::Duration::from_millis(50));
	{
		let (released, ready) = &*gate;
		*released.lock() = true;
		ready.notify_all();
	}
	assert!(
		matches!(early, Err(std::sync::mpsc::RecvTimeoutError::Timeout)),
		"the follow-bound request overtook setup: {early:?}",
	);

	let response = result_receiver
		.recv_timeout(std::time::Duration::from_secs(2))
		.expect("header request completes after follow setup")
		.expect("header response succeeds");
	assert_eq!(response.header, Some(vec![0xde, 0xad, 0xbe, 0xef]));
	let sent = provider.sent.lock().unwrap().clone();
	assert_eq!(sent.len(), 2);
	assert!(sent[0].contains("chainHead_v1_follow"));
	assert!(sent[1].contains("chainHead_v1_header"));
}

#[test]
fn header_request_binds_product_follow_alias_to_transport_follow() {
	let provider = Arc::new(ScriptedProvider::new(|request| {
		let id = extract_id(request).unwrap();
		if request.contains("chainHead_v1_follow") {
			Some(format!(r#"{{"jsonrpc":"2.0","id":"{id}","result":"REMOTE-FOLLOW"}}"#))
		} else if request.contains("chainHead_v1_header") {
			Some(format!(r#"{{"jsonrpc":"2.0","id":"{id}","result":"0xdeadbeef"}}"#))
		} else {
			None
		}
	}));
	let runtime = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let _follow_stream = runtime.remote_chain_head_follow(
		"transport-request-id".to_string(),
		RemoteChainHeadFollowRequest { genesis_hash: vec![0u8; 32], with_runtime: true },
	);
	let sent = wait_for_sent(&provider, |sent| {
		sent.iter().any(|request| request.contains("chainHead_v1_follow"))
	});
	assert!(
		sent.iter().any(|request| request.contains("chainHead_v1_follow")),
		"follow setup did not start; sent: {sent:?}",
	);

	let response = futures::executor::block_on(runtime.remote_chain_head_header(
		RemoteChainHeadHeaderRequest {
			genesis_hash: vec![0u8; 32],
			follow_subscription_id: "follow_0".to_string(),
			hash: vec![1u8; 32],
		},
	))
	.expect("product alias should resolve to the active transport follow");

	assert_eq!(response.header, Some(vec![0xde, 0xad, 0xbe, 0xef]));
	assert_eq!(provider.connect_calls.load(Ordering::SeqCst), 1);
	let sent = provider.sent.lock().unwrap().clone();
	assert_eq!(sent.len(), 2);
	assert!(sent[1].contains("chainHead_v1_header"));
}

#[test]
fn opening_a_second_follow_keeps_an_established_alias_working() {
	let follows_started = Arc::new(AtomicUsize::new(0));
	let script_follows = follows_started.clone();
	let provider = Arc::new(ScriptedProvider::new(move |request| {
		let id = extract_id(request).unwrap();
		if request.contains("chainHead_v1_follow") {
			let index = script_follows.fetch_add(1, Ordering::SeqCst);
			Some(format!(r#"{{"jsonrpc":"2.0","id":"{id}","result":"REMOTE-FOLLOW-{index}"}}"#))
		} else if request.contains("chainHead_v1_header") {
			Some(format!(r#"{{"jsonrpc":"2.0","id":"{id}","result":"0xdeadbeef"}}"#))
		} else {
			None
		}
	}));
	let runtime = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let follow_request =
		|| RemoteChainHeadFollowRequest { genesis_hash: vec![0u8; 32], with_runtime: false };
	let header_request = |follow_subscription_id: &str| RemoteChainHeadHeaderRequest {
		genesis_hash: vec![0u8; 32],
		follow_subscription_id: follow_subscription_id.to_string(),
		hash: vec![1u8; 32],
	};
	let follows_sent = |sent: &[String], count: usize| {
		sent.iter().filter(|request| request.contains("chainHead_v1_follow")).count() == count
	};

	let _first_follow =
		runtime.remote_chain_head_follow("c1:follow-one".to_string(), follow_request());
	wait_for_sent(&provider, |sent| follows_sent(sent, 1));
	futures::executor::block_on(runtime.remote_chain_head_header(header_request("c1:follow_0")))
		.expect("the first alias resolves while one follow is live");

	// Tarik's case: a second follow makes the pending-intent lookup
	// ambiguous, which must not strand the alias that already works.
	let _second_follow =
		runtime.remote_chain_head_follow("c1:follow-two".to_string(), follow_request());
	wait_for_sent(&provider, |sent| follows_sent(sent, 2));
	futures::executor::block_on(runtime.remote_chain_head_header(header_request("c1:follow_0")))
		.expect("the established alias survives a second follow on the same chain");
	futures::executor::block_on(runtime.remote_chain_head_header(header_request("c1:follow_1")))
		.expect("the second alias binds to the remaining follow");

	let headers: Vec<Value> = provider
		.sent
		.lock()
		.unwrap()
		.iter()
		.filter(|request| request.contains("chainHead_v1_header"))
		.map(|request| serde_json::from_str(request).unwrap())
		.collect();
	assert_eq!(headers.len(), 3);
	assert_eq!(headers[0]["params"][0], "REMOTE-FOLLOW-0");
	assert_eq!(
		headers[1]["params"][0], "REMOTE-FOLLOW-0",
		"the established alias stayed bound to its own follow",
	);
	assert_eq!(
		headers[2]["params"][0], "REMOTE-FOLLOW-1",
		"the new alias bound to the second follow",
	);
}

#[test]
fn pending_follow_aliases_are_product_scoped_and_ambiguous_aliases_fail_closed() {
	let provider = Arc::new(ScriptedProvider::new(|_| None));
	let runtime = ChainRuntime::new(provider, spawner_for_tests());
	let genesis_hash = vec![0u8; 32];

	runtime.register_follow_intent(&genesis_hash, "c1:request-first");
	runtime.register_follow_intent(&genesis_hash, "c2:request-first");
	assert_eq!(
		runtime
			.resolve_local_follow_id("test", &genesis_hash, "c1:follow_0",)
			.expect("c1 alias resolves within c1"),
		"c1:request-first",
	);
	assert_eq!(
		runtime
			.resolve_local_follow_id("test", &genesis_hash, "c2:follow_0",)
			.expect("c2 alias resolves within c2"),
		"c2:request-first",
	);

	runtime.register_follow_intent(&genesis_hash, "c1:request-second");
	let error = runtime
		.resolve_local_follow_id("test", &genesis_hash, "c1:follow_1")
		.expect_err("two c1 intents make an unknown alias ambiguous");
	assert_eq!(error.kind(), RuntimeFailureKind::HostFailure);
	assert!(error.reason().contains("unknown follow subscription id"));
}

#[test]
fn transaction_stop_uses_host_handle_and_accepts_finished_provider_operation() {
	let provider =
		broadcast_provider(r#""error":{"code":-32602,"message":"Invalid operation id"}"#);
	let runtime = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let genesis = vec![0x11; 32];

	let local_id = broadcast(&runtime, &genesis);
	assert!(local_id.starts_with("truapi-tx-"));
	assert_ne!(local_id, "REMOTE-OP");

	stop(&runtime, &genesis, &local_id)
		.expect("an already-finished provider operation is stopped idempotently");

	let sent = provider.sent.lock().unwrap().clone();
	let stop: Value = serde_json::from_str(
		sent.iter()
			.find(|request| request.contains("transaction_v1_stop"))
			.expect("stop request"),
	)
	.unwrap();
	assert_eq!(stop["params"][0].as_str(), Some("REMOTE-OP"));
}

#[test]
fn transaction_operations_evict_oldest_beyond_cap() {
	let runtime = ChainRuntime::new(broadcast_provider(STOPPED), spawner_for_tests());
	let genesis = vec![0x11; 32];

	let first_id = broadcast(&runtime, &genesis);
	for _ in 0..MAX_TRACKED_TRANSACTION_OPERATIONS {
		broadcast(&runtime, &genesis);
	}

	let err = stop(&runtime, &genesis, &first_id).expect_err("evicted operation id is unknown");
	assert!(err.reason().contains("unknown transaction operation id"));
}

#[test]
fn unpin_uses_typed_subxt_method_for_each_hash() {
	let provider = Arc::new(ScriptedProvider::new(|request| {
		let id = extract_id(request).unwrap();
		if request.contains("chainHead_v1_follow") {
			Some(format!(r#"{{"jsonrpc":"2.0","id":"{id}","result":"REMOTE-FOLLOW"}}"#))
		} else if request.contains("chainHead_v1_unpin") {
			Some(format!(r#"{{"jsonrpc":"2.0","id":"{id}","result":null}}"#))
		} else {
			None
		}
	}));
	let runtime = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let _follow_stream = runtime.remote_chain_head_follow(
		"local-follow".to_string(),
		RemoteChainHeadFollowRequest { genesis_hash: vec![0u8; 32], with_runtime: false },
	);
	let sent = wait_for_sent(&provider, |sent| {
		sent.iter().any(|request| request.contains("chainHead_v1_follow"))
	});
	assert!(
		sent.iter().any(|request| request.contains("chainHead_v1_follow")),
		"follow setup did not start; sent: {sent:?}",
	);

	futures::executor::block_on(runtime.remote_chain_head_unpin(RemoteChainHeadUnpinRequest {
		genesis_hash: vec![0u8; 32],
		follow_subscription_id: "local-follow".to_string(),
		hashes: vec![vec![0x11; 32], vec![0x22; 32]],
	}))
	.expect("unpin succeeds");

	let sent = provider.sent.lock().unwrap().clone();
	let unpin_requests: Vec<_> =
		sent.iter().filter(|request| request.contains("chainHead_v1_unpin")).collect();
	assert_eq!(
		unpin_requests.len(),
		2,
		"unpin should send each hash through Subxt; sent: {sent:?}",
	);
	let params = unpin_requests
		.iter()
		.map(|request| {
			let request: Value = serde_json::from_str(request).expect("json request");
			assert_eq!(request.get("method").and_then(Value::as_str), Some("chainHead_v1_unpin"),);
			request.get("params").and_then(Value::as_array).expect("params array").clone()
		})
		.collect::<Vec<_>>();
	assert_eq!(params[0][0].as_str(), Some("REMOTE-FOLLOW"));
	assert_eq!(params[1][0].as_str(), Some("REMOTE-FOLLOW"));
	assert_eq!(
		params[0][1][0].as_str(),
		Some("0x1111111111111111111111111111111111111111111111111111111111111111"),
	);
	assert_eq!(
		params[1][1][0].as_str(),
		Some("0x2222222222222222222222222222222222222222222222222222222222222222"),
	);
}

#[test]
fn header_request_rejects_unknown_follow_id_without_opening_follow() {
	let provider = Arc::new(ScriptedProvider::new(|request| {
		let id = extract_id(request).unwrap();
		if request.contains("chainHead_v1_follow") {
			Some(format!(r#"{{"jsonrpc":"2.0","id":"{id}","result":"REMOTE-FOLLOW"}}"#))
		} else if request.contains("chainHead_v1_header") {
			Some(format!(r#"{{"jsonrpc":"2.0","id":"{id}","result":"0xdeadbeef"}}"#))
		} else {
			None
		}
	}));
	let runtime = ChainRuntime::new(provider.clone(), spawner_for_tests());

	let err = futures::executor::block_on(runtime.remote_chain_head_header(
		RemoteChainHeadHeaderRequest {
			genesis_hash: vec![0u8; 32],
			follow_subscription_id: "missing-follow".to_string(),
			hash: vec![1u8; 32],
		},
	))
	.expect_err("unknown follow id should fail");

	assert_eq!(err.kind(), RuntimeFailureKind::HostFailure);
	assert!(
		err.reason().contains("unknown follow subscription id"),
		"unexpected error: {}",
		err.reason(),
	);
	assert!(provider.sent.lock().unwrap().is_empty());
}

/// Two concurrent calls for the same chain must share one provider
/// `connect` instead of racing the first connection and orphaning the
/// loser.
#[test]
fn concurrent_connection_for_shares_one_connect() {
	struct SlowConnectProvider {
		inner: ScriptedProvider,
	}

	#[async_trait::async_trait]
	impl RuntimeChainProvider for SlowConnectProvider {
		async fn connect(
			&self,
			genesis_hash: Vec<u8>,
		) -> Result<Arc<dyn JsonRpcConnection>, RuntimeFailure> {
			futures_timer::Delay::new(std::time::Duration::from_millis(50)).await;
			self.inner.connect(genesis_hash).await
		}
	}

	let provider = Arc::new(SlowConnectProvider {
		inner: ScriptedProvider::new(|request| {
			let id = extract_id(request).unwrap();
			if request.contains("chainSpec_v1_chainName") {
				Some(format!(r#"{{"jsonrpc":"2.0","id":"{id}","result":"Polkadot"}}"#))
			} else {
				None
			}
		}),
	});
	let runtime = ChainRuntime::new(provider.clone(), spawner_for_tests());

	let (first, second) = futures::executor::block_on(futures::future::join(
		runtime.remote_chain_spec_chain_name(vec![0u8; 32]),
		runtime.remote_chain_spec_chain_name(vec![0u8; 32]),
	));

	assert_eq!(first.unwrap().chain_name, "Polkadot");
	assert_eq!(second.unwrap().chain_name, "Polkadot");
	assert_eq!(provider.inner.connect_calls.load(Ordering::SeqCst), 1);
}

/// The cached Subxt bundle must pin the host-configured genesis hash
/// (never fetch it from the provider) and be built once per connection.
#[cfg_attr(target_arch = "wasm32", ignore)]
#[test]
fn subxt_connection_pins_configured_genesis_and_is_cached() {
	let provider = Arc::new(ScriptedProvider::new(|_| None));
	let runtime = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let genesis = vec![0xab; 32];

	let connection =
		futures::executor::block_on(runtime.connection_for("subxt_connection_test", &genesis))
			.expect("connection");
	let first = futures::executor::block_on(connection.subxt_connection()).expect("client");
	let second = futures::executor::block_on(connection.subxt_connection()).expect("client");

	// With the config pin, construction never asks the provider for the
	// genesis hash. The scripted provider answers nothing, so a fetch
	// would have hung instead of returning.
	assert_eq!(first.client.genesis_hash(), H256([0xab; 32]));
	assert_eq!(second.client.genesis_hash(), H256([0xab; 32]));
	let sent = provider.sent.lock().unwrap().clone();
	assert!(
		!sent.iter().any(|request| request.contains("chainSpec_v1_genesisHash")),
		"genesis hash must come from config, not the provider; sent: {sent:?}",
	);

	// One backend driver total: its follow subscription shows up once.
	let sent = wait_for_sent(&provider, |sent| {
		sent.iter().any(|request| request.contains("chainHead_v1_follow"))
	});
	assert!(
		sent.iter().any(|request| request.contains("chainHead_v1_follow")),
		"backend follow did not start; sent: {sent:?}",
	);
	std::thread::sleep(std::time::Duration::from_millis(200));
	let follows = provider
		.sent
		.lock()
		.unwrap()
		.iter()
		.filter(|request| request.contains("chainHead_v1_follow"))
		.count();
	assert_eq!(follows, 1, "cached Subxt bundle must reuse one backend follow");
}

/// When the backend driver exits (transport gone quiet for good), the
/// cached Subxt bundle must be dropped so the next caller rebuilds it.
#[cfg_attr(target_arch = "wasm32", ignore)]
#[test]
fn subxt_connection_invalidated_when_backend_driver_exits() {
	let provider = Arc::new(ScriptedProvider::new(|_| None));
	let runtime = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let genesis = vec![0xab; 32];

	let connection =
		futures::executor::block_on(runtime.connection_for("subxt_connection_test", &genesis))
			.expect("connection");
	let _client = futures::executor::block_on(connection.subxt_connection()).expect("client");
	assert!(connection.subxt_connection_setup.lock().unwrap().is_some());

	// End the response stream: the backend driver's follow stream ends,
	// the pump exits, and the exit hook must clear the cached setup.
	provider.sender.lock().unwrap().take();
	for _ in 0..500 {
		if connection.subxt_connection_setup.lock().unwrap().is_none() {
			break;
		}
		std::thread::sleep(std::time::Duration::from_millis(10));
	}
	assert!(
		connection.subxt_connection_setup.lock().unwrap().is_none(),
		"driver exit must invalidate the cached Subxt bundle",
	);
}

/// Each product runtime gets its own connection to a chain, and the core's
/// own traffic keeps one of its own. A node budgets follows and concurrent
/// operations per connection, so sharing one makes those budgets collide.
#[test]
fn each_scope_gets_its_own_connection_to_the_same_chain() {
	let provider = Arc::new(ScriptedProvider::new(|request| {
		let id = extract_id(request).unwrap();
		request
			.contains("chainSpec_v1_chainName")
			.then(|| format!(r#"{{"jsonrpc":"2.0","id":"{id}","result":"Polkadot"}}"#))
	}));
	let core = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let genesis = vec![0xab; 32];

	let first = core.for_product(1);
	let second = core.for_product(2);

	for runtime in [&core, &first, &second] {
		futures::executor::block_on(runtime.connection_for("scope_test", &genesis))
			.expect("connection");
	}

	// Same chain, three scopes, three connections.
	assert_eq!(provider.connect_calls.load(Ordering::SeqCst), 3);
	assert_eq!(core.connections.lock().unwrap().values().map(HashMap::len).sum::<usize>(), 3);
}

/// Re-scoping shares the maps rather than starting a fresh runtime, so a
/// product that asks twice reuses its own connection and does not open a
/// second one.
#[test]
fn the_same_product_reuses_its_connection() {
	let provider = Arc::new(ScriptedProvider::new(|_| None));
	let core = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let genesis = vec![0xab; 32];

	futures::executor::block_on(core.for_product(7).connection_for("scope_test", &genesis))
		.expect("first");
	futures::executor::block_on(core.for_product(7).connection_for("scope_test", &genesis))
		.expect("second");

	assert_eq!(provider.connect_calls.load(Ordering::SeqCst), 1);
	assert_eq!(core.connections.lock().unwrap().values().map(HashMap::len).sum::<usize>(), 1);
}

/// A product's follow budget is its own. Four follows for one product must
/// not consume another's, and the node counts follows per connection, so
/// the check is that no single connection carries more than four.
///
/// Counts remote follows, not local intents: `register_follow_intent`
/// records an entry before the node has acknowledged anything, so counting
/// entries alone would pass even if every follow went to one socket.
#[test]
fn a_products_follows_do_not_consume_another_scopes_budget() {
	let provider = Arc::new(ScriptedProvider::new(|request| {
		let id = extract_id(request).unwrap();
		request
			.contains("chainHead_v1_follow")
			.then(|| format!(r#"{{"jsonrpc":"2.0","id":"{id}","result":"REMOTE-{id}"}}"#))
	}));
	let core = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let genesis = vec![0xab; 32];

	let busy = core.for_product(1);
	let _busy_follows: Vec<_> = (0..4)
		.map(|n| {
			busy.remote_chain_head_follow(
				format!("c1:follow{n}"),
				RemoteChainHeadFollowRequest { genesis_hash: genesis.clone(), with_runtime: false },
			)
		})
		.collect();
	wait_for_sent(&provider, |sent| {
		sent.iter().filter(|r| r.contains("chainHead_v1_follow")).count() >= 4
	});

	let quiet = core.for_product(2);
	let _quiet_follow = quiet.remote_chain_head_follow(
		"c2:follow".to_string(),
		RemoteChainHeadFollowRequest { genesis_hash: genesis.clone(), with_runtime: false },
	);
	wait_for_sent(&provider, |sent| {
		sent.iter().filter(|r| r.contains("chainHead_v1_follow")).count() >= 5
	});

	// Each follow is established only once the node has answered, which it
	// does on the connection that asked.
	let established = |connection: &Arc<ChainConnection>| {
		connection
			.follows
			.lock()
			.unwrap()
			.values()
			.filter(|follow| follow.remote_subscription_id.is_some())
			.count()
	};
	let mut per_connection = Vec::new();
	for _ in 0..200 {
		per_connection = core
			.connections
			.lock()
			.unwrap()
			.values()
			.flat_map(|connections| connections.values())
			.filter_map(ConnectionEntry::ready)
			.map(established)
			.collect::<Vec<_>>();
		if per_connection.iter().sum::<usize>() == 5 {
			break;
		}
		std::thread::sleep(std::time::Duration::from_millis(10));
	}
	per_connection.sort_unstable();

	assert_eq!(provider.connect_calls.load(Ordering::SeqCst), 2);
	assert_eq!(
		per_connection,
		vec![1, 4],
		"five established follows did not split one per product across two connections",
	);
}

/// A scope that ends takes its connections with it. `core_instance` never
/// repeats, so without this the entry is never asked for again and the
/// cache grows for the life of the process. The smoldot provider caps
/// connections at 32, so a leak there eventually refuses every connect.
#[test]
fn a_scope_releases_its_connections_when_it_ends() {
	let provider = Arc::new(ScriptedProvider::new(|_| None));
	let core = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let genesis = vec![0xab; 32];
	let other_chain = vec![0xcd; 32];

	let product = core.for_product(1);
	for chain in [&genesis, &other_chain] {
		futures::executor::block_on(product.connection_for("release_test", chain))
			.expect("connection");
	}
	futures::executor::block_on(core.connection_for("release_test", &genesis))
		.expect("core connection");
	assert_eq!(core.connections.lock().unwrap().values().map(HashMap::len).sum::<usize>(), 3);
	remember_lost_follow(
		&core.ended_follows,
		product.scope,
		"c1:lost",
		FollowLoss::ConnectionClosed,
	);

	product.release_scope();

	// A pump finishing after the release records into nothing.
	remember_lost_follow(
		&core.ended_follows,
		product.scope,
		"c1:late",
		FollowLoss::ConnectionClosed,
	);
	assert!(
		!core.ended_follows.lock().unwrap().contains_key(&product.scope),
		"a released scope kept its record of lost follows",
	);

	let remaining: Vec<(ConnectionScope, String)> = core
		.connections
		.lock()
		.unwrap()
		.iter()
		.flat_map(|(scope, connections)| {
			connections.keys().map(|key| (*scope, key.clone())).collect::<Vec<_>>()
		})
		.collect();
	assert_eq!(
		remaining,
		vec![(ConnectionScope::Core, encode_hex(&genesis))],
		"releasing a product took the core's connection with it: {remaining:?}",
	);
}

/// Releasing a scope with a live follow must close the host's socket, not
/// just forget the connection. The follow pump holds a strong handle, so
/// without aborting it the connection stays alive and the socket stays
/// open, which is worse than leaving the entry cached.
#[test]
fn releasing_a_scope_with_a_live_follow_closes_the_socket() {
	let provider = Arc::new(ScriptedProvider::new(|request| {
		let id = extract_id(request).unwrap();
		request
			.contains("chainHead_v1_follow")
			.then(|| format!(r#"{{"jsonrpc":"2.0","id":"{id}","result":"REMOTE-FOLLOW"}}"#))
	}));
	let core = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let product = core.for_product(1);

	// Hold the stream, so nothing tears the follow down on our behalf.
	let _follow = product.remote_chain_head_follow(
		"c1:follow".to_string(),
		RemoteChainHeadFollowRequest { genesis_hash: vec![0xab; 32], with_runtime: false },
	);
	wait_for_sent(&provider, |sent| {
		sent.iter().any(|request| request.contains("chainHead_v1_follow"))
	});
	assert!(provider.sender.lock().unwrap().is_some(), "socket closed too early");

	product.release_scope();

	// The socket closes once the aborted pump drops the last handle.
	let mut closed = false;
	for _ in 0..200 {
		if provider.sender.lock().unwrap().is_none() {
			closed = true;
			break;
		}
		std::thread::sleep(std::time::Duration::from_millis(10));
	}
	assert!(closed, "the host socket stayed open after the scope was released");
}

/// A connect genuinely in flight when its scope ends must not reinstate it.
/// `core_instance` never repeats, so a reinstated entry is never asked for
/// again and never evicted. The connect is parked inside the provider until
/// after `release_scope` has run, which is the ordering that matters.
#[cfg_attr(target_arch = "wasm32", ignore)]
#[test]
fn a_connect_in_flight_cannot_resurrect_a_released_scope() {
	/// Holds every `connect` until the test lets it through.
	struct ParkedProvider {
		gate: Arc<(ParkingMutex<bool>, Condvar)>,
	}
	#[async_trait::async_trait]
	impl RuntimeChainProvider for ParkedProvider {
		async fn connect(
			&self,
			_genesis_hash: Vec<u8>,
		) -> Result<Arc<dyn JsonRpcConnection>, RuntimeFailure> {
			{
				// Scoped, so no guard is held across the await below.
				let (lock, cv) = &*self.gate;
				let mut open = lock.lock();
				while !*open {
					cv.wait(&mut open);
				}
			}
			ScriptedProvider::new(|_| None).connect(Vec::new()).await
		}
	}

	let gate = Arc::new((ParkingMutex::new(false), Condvar::new()));
	let provider = Arc::new(ParkedProvider { gate: gate.clone() });
	let core = ChainRuntime::new(provider, spawner_for_tests());
	let product = core.for_product(1);
	let genesis = vec![0xab; 32];

	// Start the connect on another thread and let it reach the provider.
	let connecting = product.clone();
	let connecting_genesis = genesis.clone();
	let handle = std::thread::spawn(move || {
		futures::executor::block_on(
			connecting.connection_for("resurrect_test", &connecting_genesis),
		)
		.is_ok()
	});
	for _ in 0..200 {
		if core
			.connections
			.lock()
			.unwrap()
			.values()
			.flat_map(|connections| connections.values())
			.filter(|entry| matches!(entry, ConnectionEntry::Connecting { .. }))
			.count() == 1
		{
			break;
		}
		std::thread::sleep(std::time::Duration::from_millis(10));
	}
	assert_eq!(
		core.connections
			.lock()
			.unwrap()
			.values()
			.flat_map(|connections| connections.values())
			.filter(|entry| matches!(entry, ConnectionEntry::Connecting { .. }))
			.count(),
		1,
		"the connect never reached the provider",
	);

	// The scope ends while that connect is still parked.
	product.release_scope();

	// Now let it finish.
	{
		let (lock, cv) = &*gate;
		*lock.lock() = true;
		cv.notify_all();
	}
	let succeeded = handle.join().expect("connect thread");

	let remaining: Vec<String> = core
		.connections
		.lock()
		.unwrap()
		.values()
		.flat_map(|connections| connections.keys())
		.cloned()
		.collect();
	assert!(
		remaining.is_empty(),
		"a connect in flight reinstated the released scope: {remaining:?}",
	);
	assert!(!succeeded, "a connect for a released scope should fail, not hand back a connection");
}

#[test]
fn releasing_a_product_drops_only_its_own_follow_intents() {
	let provider = Arc::new(ScriptedProvider::new(|_| None));
	let core = ChainRuntime::new(provider, spawner_for_tests());
	let genesis = vec![0xab; 32];
	let first = core.for_product(1);
	let second = core.for_product(2);
	first.register_follow_intent(&genesis, "c1:request");
	second.register_follow_intent(&genesis, "c2:request");

	first.release_scope();

	assert!(
		first.resolve_local_follow_id("test", &genesis, "c1:request").is_err(),
		"a released product's follow intent outlived it",
	);
	assert_eq!(
		second
			.resolve_local_follow_id("test", &genesis, "c2:request")
			.expect("c2 intent"),
		"c2:request",
	);
}

#[test]
fn a_product_cannot_resolve_another_products_follow() {
	let provider = Arc::new(ScriptedProvider::new(|_| None));
	let core = ChainRuntime::new(provider, spawner_for_tests());
	let genesis = vec![0xab; 32];
	let first = core.for_product(1);
	let second = core.for_product(2);
	first.register_follow_intent(&genesis, "host-internal");

	assert!(
		second.resolve_local_follow_id("test", &genesis, "host-internal").is_err(),
		"one product resolved another product's follow id",
	);
	assert!(
		second.resolve_local_follow_id("test", &genesis, "follow_0").is_err(),
		"one product's alias resolved to another product's follow",
	);
}

#[test]
fn a_product_cannot_stop_another_products_transaction() {
	let provider = broadcast_provider(STOPPED);
	let core = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let genesis = vec![0x11; 32];
	let owner = core.for_product(1);
	let other = core.for_product(2);

	let operation_id = broadcast(&owner, &genesis);

	let err = stop(&other, &genesis, &operation_id)
		.expect_err("another product stopped this product's transaction");
	assert!(err.reason().contains("unknown transaction operation id"));
	assert!(
		!provider
			.sent
			.lock()
			.unwrap()
			.iter()
			.any(|request| request.contains("transaction_v1_stop")),
		"the stop reached the node",
	);

	stop(&owner, &genesis, &operation_id).expect("the owner can still stop its own transaction");
}

#[test]
fn the_transaction_cap_applies_per_product() {
	let core = ChainRuntime::new(broadcast_provider(STOPPED), spawner_for_tests());
	let genesis = vec![0x11; 32];
	let quiet = core.for_product(1);
	let flooding = core.for_product(2);

	let kept = broadcast(&quiet, &genesis);
	for _ in 0..=MAX_TRACKED_TRANSACTION_OPERATIONS {
		broadcast(&flooding, &genesis);
	}

	stop(&quiet, &genesis, &kept).expect("one product's flood evicted another's operation");
}

#[test]
fn releasing_a_product_forgets_its_transactions() {
	let core = ChainRuntime::new(broadcast_provider(STOPPED), spawner_for_tests());
	let genesis = vec![0x11; 32];
	let product = core.for_product(1);
	broadcast(&product, &genesis);

	product.release_scope();

	assert!(core.transaction_operations.lock().unwrap().is_empty());
}

/// A call that lands after `release_scope` leaves nothing behind, taken
/// through the public follow rather than the scope-group helpers, because
/// the public path writes `follow_starts` outside them. The spawner is
/// inert so the setup task never runs: its failure path would clear the
/// same entries and hide a missing release gate.
#[test]
fn a_call_finishing_after_release_leaves_nothing_behind() {
	let inert: Spawner = Arc::new(|_task| {});
	let core = ChainRuntime::new(broadcast_provider(STOPPED), inert);
	let genesis = vec![0x11; 32];
	let product = core.for_product(1);
	product.release_scope();

	let _follow = product.remote_chain_head_follow(
		"c1:late".to_owned(),
		RemoteChainHeadFollowRequest { genesis_hash: genesis.clone(), with_runtime: false },
	);
	product.with_transaction_operations(|operations| {
		operations.insert(
			1,
			TransactionOperation {
				genesis_hash: genesis.clone(),
				provider_operation_id: "REMOTE-OP".to_owned(),
			},
		)
	});

	assert!(core.follow_intents.lock().unwrap().is_empty());
	assert!(core.follow_starts.lock().is_empty(), "a released scope kept its follow setup");
	assert!(core.transaction_operations.lock().unwrap().is_empty());
}

#[test]
fn releasing_a_product_sweeps_only_its_own_follow_starts() {
	let core = ChainRuntime::new(broadcast_provider(STOPPED), spawner_for_tests());
	let genesis = vec![0xab; 32];
	let first = core.for_product(1);
	let second = core.for_product(2);
	for (runtime, follow_id) in [(&first, "c1:request"), (&second, "c2:request")] {
		runtime.register_follow_intent(&genesis, follow_id);
		let started: LocalFollowStart = futures::future::ready(Ok(())).boxed().shared();
		core.follow_starts
			.lock()
			.insert((genesis.clone(), follow_id.to_owned()), started);
	}

	first.release_scope();

	let remaining: Vec<String> = core
		.follow_starts
		.lock()
		.keys()
		.map(|(_, follow_id)| follow_id.clone())
		.collect();
	assert_eq!(remaining, vec!["c2:request".to_owned()]);
}

/// Releasing one product leaves another product's connections alone.
#[test]
fn releasing_one_product_does_not_touch_another() {
	let provider = Arc::new(ScriptedProvider::new(|_| None));
	let core = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let genesis = vec![0xab; 32];

	let first = core.for_product(1);
	let second = core.for_product(2);
	for runtime in [&first, &second] {
		futures::executor::block_on(runtime.connection_for("release_test", &genesis))
			.expect("connection");
	}

	first.release_scope();

	let remaining: Vec<(ConnectionScope, String)> = core
		.connections
		.lock()
		.unwrap()
		.iter()
		.flat_map(|(scope, connections)| {
			connections.keys().map(|key| (*scope, key.clone())).collect::<Vec<_>>()
		})
		.collect();
	assert_eq!(remaining, vec![(ConnectionScope::Product(2), encode_hex(&genesis))]);
}

#[test]
fn a_queued_follow_cannot_open_a_connection_after_product_release() {
	let queued = Arc::new(Mutex::new(Vec::<BoxFuture<'static, ()>>::new()));
	let queue = queued.clone();
	let spawner: Spawner = Arc::new(move |task| queue.lock().unwrap().push(task));
	let provider = Arc::new(ScriptedProvider::new(|_| None));
	let core = ChainRuntime::new(provider, spawner);
	let product = core.for_product(1);
	let mut follow = product.remote_chain_head_follow(
		"c1:queued".to_string(),
		RemoteChainHeadFollowRequest { genesis_hash: vec![0xab; 32], with_runtime: false },
	);
	let task = queued.lock().unwrap().pop().expect("queued follow setup");

	product.release_scope();
	let completed = task.now_or_never();
	let connections_empty =
		!core.connections.lock().unwrap().contains_key(&ConnectionScope::Product(1));

	assert_eq!(
		(completed, connections_empty, follow.next().now_or_never()),
		(Some(()), true, Some(Some(Err(scope_released(FOLLOW_METHOD)))),),
		"closing the product must reject follow setup queued before teardown",
	);
}

#[test]
fn a_failed_connection_can_be_retried() {
	struct FailOnceProvider {
		attempted: AtomicBool,
		inner: ScriptedProvider,
	}

	#[async_trait::async_trait]
	impl RuntimeChainProvider for FailOnceProvider {
		async fn connect(
			&self,
			genesis_hash: Vec<u8>,
		) -> Result<Arc<dyn JsonRpcConnection>, RuntimeFailure> {
			if !self.attempted.swap(true, Ordering::SeqCst) {
				return Err(RuntimeFailure::unavailable("remote_chain_connect"));
			}
			self.inner.connect(genesis_hash).await
		}
	}

	let provider = Arc::new(FailOnceProvider {
		attempted: AtomicBool::new(false),
		inner: ScriptedProvider::new(|_| None),
	});
	let runtime = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let genesis = vec![0xab; 32];
	let failed = futures::executor::block_on(runtime.connection_for("retry", &genesis)).map(|_| ());
	let retried =
		futures::executor::block_on(runtime.connection_for("retry", &genesis)).map(|_| ());
	assert_eq!(
		(failed, retried, provider.inner.connect_calls.load(Ordering::SeqCst)),
		(Err(RuntimeFailure::unavailable_with_reason("retry", "remote_chain_connect")), Ok(()), 1),
	);
}

#[test]
fn closed_products_do_not_accumulate_in_the_connection_registry() {
	let provider = Arc::new(ScriptedProvider::new(|_| None));
	let core = ChainRuntime::new(provider, spawner_for_tests());
	for instance in 1..=1_000 {
		core.for_product(instance).release_scope();
	}
	let remaining: Vec<_> = core.connections.lock().unwrap().keys().copied().collect();
	assert_eq!(remaining, vec![ConnectionScope::Core]);
}

#[test]
fn unknown_genesis_chain_spec_propagates_failure() {
	let provider = Arc::new(UnavailableChainProvider);
	let runtime = ChainRuntime::new(provider, spawner_for_tests());
	let err = match futures::executor::block_on(runtime.remote_chain_spec_chain_name(vec![0u8; 32]))
	{
		Ok(_) => panic!("expected failure"),
		Err(err) => err,
	};
	assert_eq!(err.kind(), RuntimeFailureKind::Unavailable);
	assert_eq!(err.method(), "remote_chain_spec_chain_name");
}

/// A socket that drops under a call keeps the reason the transport gave, so the
/// product and the host log can tell it apart from a node that answered
/// with an error.
#[test]
fn dropped_connection_keeps_the_transport_reason() {
	let provider = Arc::new(ScriptedProvider::new(|_| None));
	provider.sender.lock().unwrap().take();
	let runtime = ChainRuntime::new(provider, spawner_for_tests());
	let err =
		match futures::executor::block_on(runtime.remote_chain_spec_genesis_hash(vec![0u8; 32])) {
			Ok(_) => panic!("expected failure"),
			Err(err) => err,
		};
	assert_eq!(err.kind(), RuntimeFailureKind::Unavailable);
	assert!(
		err.reason()
			.starts_with("remote_chain_spec_genesis_hash: RPC error: client error: json-rpc"),
		"unexpected reason: {}",
		err.reason()
	);
}

/// A transport that is reconnecting is unavailable rather than broken, and
/// its reason still reaches the product and the host log.
#[test]
fn reconnecting_transport_keeps_its_reason() {
	let failure = rpc_failure(
		"get_head_header",
		SubxtRpcError::DisconnectedWillReconnect("socket closed".to_string()),
	);
	assert_eq!(failure.kind(), RuntimeFailureKind::Unavailable);
	assert_eq!(failure.method(), "get_head_header");
	assert!(failure.reason().contains("socket closed"), "unexpected reason: {}", failure.reason());
}

#[test]
fn json_rpc_error_becomes_host_failure() {
	let provider = Arc::new(ScriptedProvider::new(|request| {
		let id = extract_id(request).unwrap();
		Some(format!(
			r#"{{"jsonrpc":"2.0","id":"{id}","error":{{"code":-32601,"message":"method not found"}}}}"#
		))
	}));
	let runtime = ChainRuntime::new(provider, spawner_for_tests());
	let err = match futures::executor::block_on(runtime.remote_chain_spec_chain_name(vec![0u8; 32]))
	{
		Ok(_) => panic!("expected failure"),
		Err(err) => err,
	};
	assert_eq!(err.kind(), RuntimeFailureKind::HostFailure);
	assert!(err.reason().contains("method not found"), "unexpected reason: {}", err.reason());
}

/// A product only sees the reason string, so a transport that closes under
/// a chain-head call must say so instead of naming just the method.
#[test]
fn a_closed_transport_names_its_cause_on_the_call_and_the_follow() {
	let provider = Arc::new(ScriptedProvider::new(|request| {
		let id = extract_id(request).unwrap();
		request
			.contains("chainHead_v1_follow")
			.then(|| format!(r#"{{"jsonrpc":"2.0","id":"{id}","result":"REMOTE-FOLLOW"}}"#))
	}));
	let runtime = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let mut follow = runtime.remote_chain_head_follow(
		"local-follow".to_string(),
		RemoteChainHeadFollowRequest { genesis_hash: vec![0u8; 32], with_runtime: false },
	);
	notification_sender(&provider)
		.unbounded_send(
			r#"{"jsonrpc":"2.0","method":"chainHead_v1_followEvent","params":{"subscription":"REMOTE-FOLLOW","result":{"event":"initialized","finalizedBlockHashes":[]}}}"#
				.to_string(),
		)
		.unwrap();
	assert!(matches!(
		futures::executor::block_on(follow.next()),
		Some(Ok(RemoteChainHeadFollowItem::Initialized { .. }))
	));
	let storage_runtime = runtime.clone();
	let storage = std::thread::spawn(move || {
		futures::executor::block_on(storage_runtime.remote_chain_head_storage(
			RemoteChainHeadStorageRequest {
				genesis_hash: vec![0u8; 32],
				follow_subscription_id: "local-follow".to_string(),
				hash: vec![1u8; 32],
				items: vec![StorageQueryItem {
					key: vec![0xaa],
					query_type: StorageQueryType::Value,
				}],
				child_trie: None,
			},
		))
	});
	wait_for_sent(&provider, |sent| {
		sent.iter().any(|request| request.contains("chainHead_v1_storage"))
	});

	provider.sender.lock().unwrap().take();

	let cause = "RPC error: client error: json-rpc connection closed: RPC error: client error: json-rpc response stream ended";
	let storage_failure = storage.join().unwrap().map(|_| ()).map_err(|failure| failure.reason());
	let follow_failures: Vec<String> = futures::executor::block_on(
		follow.filter_map(|item| core::future::ready(item.err())).collect::<Vec<_>>(),
	)
	.into_iter()
	.map(|failure| failure.reason())
	.collect();
	assert_eq!(
		(storage_failure, follow_failures),
		(
			Err(format!("remote_chain_head_storage: {cause}")),
			vec![format!("remote_chain_head_follow: {cause}")]
		),
	);
}

#[test]
fn follow_event_initialized_translates_to_v01_item() {
	// Answer `chainHead_v1_follow` through the synchronized responder so
	// the ack cannot reach the response loop before the pending request
	// is registered.
	let provider = Arc::new(ScriptedProvider::new(|request| {
		let id = extract_id(request).unwrap();
		if request.contains("chainHead_v1_follow") {
			Some(format!(r#"{{"jsonrpc":"2.0","id":"{id}","result":"REMOTE-FOLLOW"}}"#))
		} else {
			None
		}
	}));
	let runtime = ChainRuntime::new(provider.clone(), spawner_for_tests());

	let mut stream = runtime.remote_chain_head_follow(
		"local-follow".to_string(),
		RemoteChainHeadFollowRequest { genesis_hash: vec![0u8; 32], with_runtime: false },
	);

	// Push follow events keyed by remote subscription id. Events that
	// land before the follow ack are buffered by remote id and replayed
	// once the follow is established.
	let tx = notification_sender(&provider);
	tx.unbounded_send(
        r#"{"jsonrpc":"2.0","method":"chainHead_v1_followEvent","params":{"subscription":"REMOTE-FOLLOW","result":{"event":"initialized","finalizedBlockHashes":["0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]}}}"#
            .to_string(),
    ).unwrap();
	tx.unbounded_send(
        r#"{"jsonrpc":"2.0","method":"chainHead_v1_followEvent","params":{"subscription":"REMOTE-FOLLOW","result":{"event":"stop"}}}"#
            .to_string(),
    ).unwrap();

	let items: Vec<_> = futures::executor::block_on(async {
		let mut out = Vec::new();
		while let Some(item) = stream.next().await {
			let is_stop = matches!(item, Ok(RemoteChainHeadFollowItem::Stop));
			out.push(item);
			if is_stop {
				break;
			}
		}
		out
	});

	match &items[0] {
		Ok(RemoteChainHeadFollowItem::Initialized {
			finalized_block_hashes,
			finalized_block_runtime,
		}) => {
			assert_eq!(finalized_block_hashes, &vec![vec![0xaa; 32]]);
			assert!(finalized_block_runtime.is_none());
		},
		other => panic!("expected Initialized, got {other:?}"),
	}
	assert!(matches!(items[1], Ok(RemoteChainHeadFollowItem::Stop)));
}

/// A follow whose setup never reaches the chain must end with that
/// failure. Ending without one reaches the product as `complete`, which
/// reads as a subscription that simply had nothing more to say.
#[test]
fn follow_setup_failure_ends_the_stream_with_the_failure() {
	let runtime = ChainRuntime::new(Arc::new(UnavailableChainProvider), spawner_for_tests());

	let mut stream = runtime.remote_chain_head_follow(
		"local-follow".to_string(),
		RemoteChainHeadFollowRequest { genesis_hash: vec![0u8; 32], with_runtime: false },
	);

	let failure = match futures::executor::block_on(stream.next()) {
		Some(Err(failure)) => failure,
		other => panic!("expected the setup failure, got {other:?}"),
	};
	assert_eq!(failure.kind(), RuntimeFailureKind::Unavailable);
	assert_eq!(failure.method(), FOLLOW_METHOD);
	assert!(futures::executor::block_on(stream.next()).is_none());
}

/// An upstream event the follow cannot read ends the stream with a
/// failure, after the items that arrived before it.
#[cfg_attr(target_arch = "wasm32", ignore)]
#[test]
fn an_unreadable_follow_event_interrupts_the_stream_after_earlier_items() {
	let provider = Arc::new(ScriptedProvider::new(|request| {
		let id = extract_id(request).unwrap();
		if request.contains("chainHead_v1_follow") {
			Some(format!(r#"{{"jsonrpc":"2.0","id":"{id}","result":"REMOTE-FOLLOW"}}"#))
		} else {
			None
		}
	}));
	let runtime = ChainRuntime::new(provider.clone(), spawner_for_tests());

	let mut stream = runtime.remote_chain_head_follow(
		"local-follow".to_string(),
		RemoteChainHeadFollowRequest { genesis_hash: vec![0u8; 32], with_runtime: false },
	);

	let tx = notification_sender(&provider);
	tx.unbounded_send(
        r#"{"jsonrpc":"2.0","method":"chainHead_v1_followEvent","params":{"subscription":"REMOTE-FOLLOW","result":{"event":"initialized","finalizedBlockHashes":["0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]}}}"#
            .to_string(),
    ).unwrap();
	// `newBlock` without its `parentBlockHash`: an event the follow
	// cannot read.
	tx.unbounded_send(
        r#"{"jsonrpc":"2.0","method":"chainHead_v1_followEvent","params":{"subscription":"REMOTE-FOLLOW","result":{"event":"newBlock","blockHash":"0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}}}"#
            .to_string(),
    ).unwrap();

	assert!(matches!(
		futures::executor::block_on(stream.next()),
		Some(Ok(RemoteChainHeadFollowItem::Initialized { .. }))
	));
	let failure = match futures::executor::block_on(stream.next()) {
		Some(Err(failure)) => failure,
		other => panic!("expected the follow to be interrupted, got {other:?}"),
	};
	assert_eq!(failure.method(), FOLLOW_METHOD);
	assert!(futures::executor::block_on(stream.next()).is_none());
}

#[cfg_attr(target_arch = "wasm32", ignore)]
#[test]
fn drop_follow_stream_sends_unfollow() {
	let provider = Arc::new(ScriptedProvider::new(|request| {
		let id = extract_id(request).unwrap();
		if request.contains("chainHead_v1_follow") {
			Some(format!(r#"{{"jsonrpc":"2.0","id":"{id}","result":"REMOTE-FOLLOW"}}"#))
		} else {
			None
		}
	}));
	let runtime = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let sent = provider.sent.clone();

	let stream = runtime.remote_chain_head_follow(
		"local-follow".to_string(),
		RemoteChainHeadFollowRequest { genesis_hash: vec![0u8; 32], with_runtime: false },
	);

	// Wait until the follow setup roundtrips and lands in `sent`.
	// Generous timeout so the test stays robust under loaded CI runners
	// where the spawner can be slow to schedule the request task.
	for _ in 0..500 {
		if !sent.lock().unwrap().is_empty() {
			break;
		}
		std::thread::sleep(std::time::Duration::from_millis(10));
	}

	drop(stream);

	// Wait for the cleanup task to run and emit the unfollow request.
	for _ in 0..500 {
		if sent.lock().unwrap().len() >= 2 {
			break;
		}
		std::thread::sleep(std::time::Duration::from_millis(10));
	}

	let messages = sent.lock().unwrap().clone();
	assert!(
		messages.iter().any(|m| m.contains("chainHead_v1_unfollow")),
		"unfollow not sent; messages: {messages:?}",
	);
}

/// Only follows the node acknowledged hold a slot in its budget. A follow
/// recorded on intent has asked for nothing yet, so counting it would
/// report a connection as full while the node still had room.
#[test]
fn only_acknowledged_follows_hold_a_slot_in_the_budget() {
	let mut follows: HashMap<String, FollowState> = HashMap::new();
	let intended = |remote: Option<&str>| FollowState {
		with_runtime: false,
		client_subscription_id: None,
		remote_subscription_id: remote.map(str::to_string),
		abort: None,
		sender: mpsc::unbounded().0,
		cancelled: Arc::new(AtomicBool::new(false)),
		activity: FollowActivity::new(),
	};
	follows.insert("acknowledged-b".to_string(), intended(Some("REMOTE-B")));
	follows.insert("intent-only".to_string(), intended(None));
	follows.insert("acknowledged-a".to_string(), intended(Some("REMOTE-A")));

	assert_eq!(
		ChainConnection::follows_holding_a_slot(&follows),
		vec!["acknowledged-a".to_string(), "acknowledged-b".to_string()],
		"an unacknowledged follow occupies nothing on the node",
	);
	assert!(
		ChainConnection::follows_holding_a_slot(&follows).len() < FOLLOWS_A_NODE_BUDGETS,
		"two acknowledged follows are below the budget",
	);
}

/// Chain the lost-follow tests read from.
const LOST_FOLLOW_CHAIN: [u8; 32] = [7u8; 32];

/// A provider that acknowledges every follow and starts every storage read.
fn follow_and_storage_provider() -> Arc<ScriptedProvider> {
	Arc::new(ScriptedProvider::new(|request| {
		let id = extract_id(request)?;
		if request.contains("chainHead_v1_follow") {
			Some(format!(r#"{{"jsonrpc":"2.0","id":"{id}","result":"REMOTE-FOLLOW"}}"#))
		} else if request.contains("chainHead_v1_storage") {
			Some(format!(
				r#"{{"jsonrpc":"2.0","id":"{id}","result":{{"result":"started","operationId":"OP"}}}}"#
			))
		} else {
			None
		}
	}))
}

/// One storage read bound to `follow_subscription_id`.
fn read_storage(
	runtime: &ChainRuntime,
	follow_subscription_id: &str,
) -> Result<RemoteChainHeadStorageResponse, RuntimeFailure> {
	futures::executor::block_on(runtime.remote_chain_head_storage(RemoteChainHeadStorageRequest {
		genesis_hash: LOST_FOLLOW_CHAIN.to_vec(),
		follow_subscription_id: follow_subscription_id.to_string(),
		hash: vec![1u8; 32],
		items: vec![StorageQueryItem { key: vec![0xaa], query_type: StorageQueryType::Value }],
		child_trie: None,
	}))
}

/// Sends `event` as a follow event of the scripted remote follow.
fn send_follow_event(provider: &ScriptedProvider, event: &str) {
	notification_sender(provider)
		.unbounded_send(format!(
			r#"{{"jsonrpc":"2.0","method":"chainHead_v1_followEvent","params":{{"subscription":"REMOTE-FOLLOW","result":{event}}}}}"#
		))
		.unwrap();
}

/// Opens `follow_subscription_id` and waits until the node has initialized it.
fn open_initialized_follow(
	runtime: &ChainRuntime,
	provider: &ScriptedProvider,
	follow_subscription_id: &str,
) -> BoxStream<'static, Result<RemoteChainHeadFollowItem, RuntimeFailure>> {
	let mut follow = runtime.remote_chain_head_follow(
		follow_subscription_id.to_string(),
		RemoteChainHeadFollowRequest {
			genesis_hash: LOST_FOLLOW_CHAIN.to_vec(),
			with_runtime: false,
		},
	);
	wait_for_sent(provider, |sent| {
		sent.iter().any(|request| request.contains("chainHead_v1_follow"))
	});
	send_follow_event(provider, r#"{"event":"initialized","finalizedBlockHashes":[]}"#);
	assert!(matches!(
		futures::executor::block_on(follow.next()),
		Some(Ok(RemoteChainHeadFollowItem::Initialized { .. }))
	));
	follow
}

/// Kills the transport, then lets the caller drop its stream once it has
/// drained, which clears the scope's intent and leaves the id with no owner.
fn lose_connection(
	provider: &ScriptedProvider,
	mut follow: BoxStream<'static, Result<RemoteChainHeadFollowItem, RuntimeFailure>>,
) {
	provider.sender.lock().unwrap().take();
	let _ = futures::executor::block_on(
		follow
			.by_ref()
			.filter_map(|item| core::future::ready(item.err()))
			.collect::<Vec<_>>(),
	);
}

/// A follow lost with its transport is named as such, even after the
/// caller has dropped the stream and the scope has forgotten the intent.
/// That is the path a product takes, so it is the one that has to answer.
#[test]
fn a_follow_lost_with_its_connection_is_named_after_the_caller_let_go() {
	let provider = follow_and_storage_provider();
	let runtime = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let follow = open_initialized_follow(&runtime, &provider, "host:1");

	lose_connection(&provider, follow);

	let failure =
		read_storage(&runtime, "host:1").expect_err("a read on the lost follow must fail");
	assert_eq!(failure.kind(), RuntimeFailureKind::Unavailable);
	assert_eq!(
		failure.reason(),
		"remote_chain_head_storage: follow subscription \"host:1\" ended when its connection closed",
	);
}

/// A product that minted its own alias keeps calling with it after the
/// loss, so the alias has to answer with the cause, and must not bind to
/// whichever follow is unaliased next.
#[test]
fn a_lost_follow_is_named_under_the_alias_a_product_bound() {
	let provider = follow_and_storage_provider();
	let runtime = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let follow = open_initialized_follow(&runtime, &provider, "host:1");
	read_storage(&runtime, "follow_0").expect("the alias binds to the live follow");

	lose_connection(&provider, follow);

	let failure =
		read_storage(&runtime, "follow_0").expect_err("a read on the lost alias must fail");
	assert_eq!(
		failure.reason(),
		"remote_chain_head_storage: follow subscription \"follow_0\" ended when its connection closed",
	);
}

/// An id this runtime never issued is still reported as unknown, so the
/// message above means what it says.
#[test]
fn an_id_that_was_never_followed_is_still_unknown() {
	let runtime = ChainRuntime::new(Arc::new(ScriptedProvider::new(|_| None)), spawner_for_tests());
	let failure = read_storage(&runtime, "never-issued").expect_err("an unknown id must fail");
	assert!(
		failure.reason().contains("unknown follow subscription id"),
		"unexpected reason: {}",
		failure.reason()
	);
}

/// A node can end a follow with a `stop` event while the connection stays
/// up. The caller is left holding the same spent id as after a transport
/// cut, so it has to be told what happened rather than that the id was
/// never issued.
#[test]
fn a_node_stop_names_itself_on_the_next_call() {
	let provider = follow_and_storage_provider();
	let runtime = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let mut follow = open_initialized_follow(&runtime, &provider, "host:1");
	assert!(read_storage(&runtime, "host:1").is_ok(), "the read before the stop should succeed");

	send_follow_event(&provider, r#"{"event":"stop"}"#);
	assert!(matches!(
		futures::executor::block_on(follow.next()),
		Some(Ok(RemoteChainHeadFollowItem::Stop))
	));
	drop(follow);

	let failure = read_storage(&runtime, "host:1")
		.expect_err("the follow is gone, so the read cannot succeed");
	assert!(
		failure.reason().contains("was stopped by the node"),
		"a node stop should name itself, got: {}",
		failure.reason(),
	);
}

/// Resolving an unseen id against the sole unaliased follow is how a
/// product-minted alias binds, but an id already known to be spent is not
/// unseen. Answering it from an unrelated follow would both hide the loss
/// and serve a block the follow never pinned.
#[test]
fn a_spent_follow_id_is_not_served_by_an_unrelated_follow() {
	let provider = Arc::new(ScriptedProvider::new(|request| {
		let id = extract_id(request).unwrap();
		if request.contains("chainHead_v1_follow") {
			Some(format!(r#"{{"jsonrpc":"2.0","id":"{id}","result":"REMOTE-FOLLOW"}}"#))
		} else if request.contains("chainHead_v1_header") {
			Some(format!(r#"{{"jsonrpc":"2.0","id":"{id}","result":"0xdeadbeef"}}"#))
		} else {
			None
		}
	}));
	let runtime = ChainRuntime::new(provider.clone(), spawner_for_tests());

	// One live follow, never aliased, exactly as a fresh refollow leaves it.
	let _follow = runtime.remote_chain_head_follow(
		"host:fresh".to_string(),
		RemoteChainHeadFollowRequest { genesis_hash: vec![0u8; 32], with_runtime: true },
	);
	wait_for_sent(&provider, |sent| {
		sent.iter().any(|request| request.contains("chainHead_v1_follow"))
	});
	remember_lost_follow(
		&runtime.ended_follows,
		runtime.scope,
		"host:dead",
		FollowLoss::ConnectionClosed,
	);

	let failure = futures::executor::block_on(runtime.remote_chain_head_header(
		RemoteChainHeadHeaderRequest {
			genesis_hash: vec![0u8; 32],
			follow_subscription_id: "host:dead".to_string(),
			hash: vec![1u8; 32],
		},
	))
	.expect_err("a spent id must not be answered by the live follow");
	assert!(
		failure.reason().contains("ended when its connection closed"),
		"the spent id should report its loss, got: {}",
		failure.reason(),
	);

	// The binding path itself still works for an id that is merely unseen.
	let response = futures::executor::block_on(runtime.remote_chain_head_header(
		RemoteChainHeadHeaderRequest {
			genesis_hash: vec![0u8; 32],
			follow_subscription_id: "follow_0".to_string(),
			hash: vec![1u8; 32],
		},
	))
	.expect("an unseen product alias still binds to the live follow");
	assert_eq!(response.header, Some(vec![0xde, 0xad, 0xbe, 0xef]));
}

#[test]
fn encode_hex_round_trip() {
	let bytes = vec![0x00u8, 0x12, 0xab, 0xff];
	let s = encode_hex(&bytes);
	assert_eq!(s, "0x0012abff");
	assert_eq!(decode_hex(&s).unwrap(), bytes);
}

#[test]
fn parse_runtime_type_valid_sorts_apis() {
	let runtime_type =
		map_runtime_event(subxt_chain::RuntimeEvent::Valid(subxt_chain::RuntimeVersionEvent {
			spec: subxt_chain::RuntimeSpec {
				spec_name: "polkadot".to_string(),
				impl_name: "parity-polkadot".to_string(),
				spec_version: 1000,
				impl_version: 1,
				transaction_version: 24,
				apis: HashMap::from([("0xbeef".to_string(), 2), ("0xbabe".to_string(), 4)]),
			},
		}))
		.unwrap();
	match runtime_type {
		RuntimeType::Valid(spec) => {
			assert_eq!(spec.apis.len(), 2);
			assert_eq!(spec.apis[0].name, "0xbabe");
			assert_eq!(spec.apis[1].name, "0xbeef");
			assert_eq!(spec.transaction_version, Some(24));
		},
		other => panic!("expected Valid, got {other:?}"),
	}
}

/// The block every scripted follow reports as finalized.
const RUNTIME_BLOCK: [u8; 32] = [0x11; 32];
/// A later block that runs the same code as `RUNTIME_BLOCK`.
const LATER_BLOCK: [u8; 32] = [0x22; 32];
/// A block after a runtime upgrade.
const UPGRADED_BLOCK: [u8; 32] = [0x33; 32];
const SHARED_GENESIS: [u8; 32] = [0xab; 32];

/// The hash of the runtime code at `block` on a chain upgraded at
/// `UPGRADED_BLOCK`.
fn code_hash_with_upgrade(block: &[u8]) -> Option<[u8; 32]> {
	Some(if block == UPGRADED_BLOCK { [0xc2; 32] } else { [0xc1; 32] })
}

fn follow_event(result: Value) -> String {
	follow_event_on("REMOTE-FOLLOW", result)
}

fn follow_event_on(subscription: &str, result: Value) -> String {
	serde_json::json!({
		"jsonrpc": "2.0",
		"method": "chainHead_v1_followEvent",
		"params": {"subscription": subscription, "result": result},
	})
	.to_string()
}

fn call_done_event(operation_id: &str) -> String {
	follow_event(serde_json::json!({
		"event": "operationCallDone",
		"operationId": operation_id,
		"output": "0x0102",
	}))
}

/// A node whose follows start at `RUNTIME_BLOCK`, whose blocks follow
/// [`code_hash_with_upgrade`], and whose calls start as `call-<n>`. With
/// `answer_calls` each call is reported done right after it starts.
fn runtime_call_node(answer_calls: bool) -> Arc<ScriptedProvider> {
	runtime_call_node_with(answer_calls, code_hash_with_upgrade)
}

/// As [`runtime_call_node`], with `code_hash` giving the hash of the runtime
/// code at each block, or none when the node cannot tell.
fn runtime_call_node_with(
	answer_calls: bool,
	code_hash: fn(&[u8]) -> Option<[u8; 32]>,
) -> Arc<ScriptedProvider> {
	Arc::new(ScriptedProvider::with_frames(runtime_call_frames(answer_calls, code_hash)))
}

/// As [`runtime_call_node`] reporting every call done, except that the
/// first request for each method in `unanswered` gets no reply, so the test
/// can answer it later.
fn runtime_call_node_withholding(unanswered: &[&'static str]) -> Arc<ScriptedProvider> {
	let answer = runtime_call_frames(true, code_hash_with_upgrade);
	let unanswered = Mutex::new(unanswered.to_vec());
	Arc::new(ScriptedProvider::with_frames(move |request| {
		let mut unanswered = unanswered.lock().unwrap();
		match unanswered.iter().position(|method| request.contains(method)) {
			Some(first) => {
				unanswered.remove(first);
				Vec::new()
			},
			None => answer(request),
		}
	}))
}

/// The frames the node of [`runtime_call_node_with`] answers a request with.
fn runtime_call_frames(
	answer_calls: bool,
	code_hash: fn(&[u8]) -> Option<[u8; 32]>,
) -> impl Fn(&str) -> Vec<String> + Send + Sync + 'static {
	let calls = Arc::new(AtomicUsize::new(0));
	move |request| {
		let id = extract_id(request).unwrap();
		let request: Value = serde_json::from_str(request).unwrap();
		match request["method"].as_str() {
			Some("chainHead_v1_follow") => {
				vec![
					format!(r#"{{"jsonrpc":"2.0","id":"{id}","result":"REMOTE-FOLLOW"}}"#),
					follow_event(serde_json::json!({
						"event": "initialized",
						"finalizedBlockHashes": [format!("0x{}", hex::encode(RUNTIME_BLOCK))],
						"finalizedBlockRuntime": {"type": "valid", "spec": {
							"specName": "people",
							"implName": "people",
							"specVersion": 1,
							"implVersion": 1,
							"transactionVersion": 1,
							"apis": {},
						}},
					})),
				]
			},
			Some("chainHead_v1_call") => {
				let operation_id = format!("call-{}", calls.fetch_add(1, Ordering::SeqCst) + 1);
				let started = format!(
					r#"{{"jsonrpc":"2.0","id":"{id}","result":{{"result":"started","operationId":"{operation_id}"}}}}"#
				);
				let mut frames = vec![started];
				if answer_calls {
					frames.push(call_done_event(&operation_id));
				}
				frames
			},
			Some("chainHead_v1_stopOperation") => {
				vec![format!(r#"{{"jsonrpc":"2.0","id":"{id}","result":null}}"#)]
			},
			Some("state_getStorageHash") => {
				let block = request["params"][1].as_str().unwrap().trim_start_matches("0x");
				let result = match code_hash(&hex::decode(block).unwrap()) {
					Some(hash) => format!(r#""0x{}""#, hex::encode(hash)),
					None => "null".to_string(),
				};
				vec![format!(r#"{{"jsonrpc":"2.0","id":"{id}","result":{result}}}"#)]
			},
			_ => Vec::new(),
		}
	}
}

type FollowItems = BoxStream<'static, Result<RemoteChainHeadFollowItem, RuntimeFailure>>;

/// The next item of `follow` that `pick` accepts, if one arrives within
/// `within`.
fn next_matching(
	follow: &mut FollowItems,
	within: Duration,
	pick: impl Fn(&RemoteChainHeadFollowItem) -> bool,
) -> Option<RemoteChainHeadFollowItem> {
	futures::executor::block_on(async {
		let deadline = futures_timer::Delay::new(within).fuse();
		pin_mut!(deadline);
		loop {
			let next = follow.next().fuse();
			pin_mut!(next);
			futures::select! {
				item = next => match item {
					Some(Ok(item)) if pick(&item) => return Some(item),
					Some(_) => {},
					None => return None,
				},
				() = deadline => return None,
			}
		}
	})
}

fn is_call_end(item: &RemoteChainHeadFollowItem) -> bool {
	matches!(
		item,
		RemoteChainHeadFollowItem::OperationCallDone { .. } |
			RemoteChainHeadFollowItem::OperationInaccessible { .. }
	)
}

/// A product scope following the shared chain, once its follow has started.
/// Products follow one after another, so the first owns the first
/// connection the node opened.
fn following_product(core: &ChainRuntime, instance: u64) -> (ChainRuntime, FollowItems) {
	let product = core.for_product(instance);
	let mut follow = product.remote_chain_head_follow(
		format!("c{instance}:follow"),
		RemoteChainHeadFollowRequest { genesis_hash: SHARED_GENESIS.to_vec(), with_runtime: true },
	);
	next_matching(&mut follow, Duration::from_secs(5), |item| {
		matches!(item, RemoteChainHeadFollowItem::Initialized { .. })
	})
	.expect("the follow starts");
	(product, follow)
}

fn call(product: &ChainRuntime, instance: u64, function: &str, block: [u8; 32]) -> String {
	call_with(product, instance, function, block, vec![15, 0, 0, 0])
}

fn call_with(
	product: &ChainRuntime,
	instance: u64,
	function: &str,
	block: [u8; 32],
	call_parameters: Vec<u8>,
) -> String {
	let response = futures::executor::block_on(product.remote_chain_head_call(call_request(
		instance,
		function,
		block,
		call_parameters,
	)))
	.expect("the call starts");
	match response.operation {
		OperationStartedResult::Started { operation_id } => operation_id,
		OperationStartedResult::LimitReached => panic!("the call was refused"),
	}
}

fn call_request(
	instance: u64,
	function: &str,
	block: [u8; 32],
	call_parameters: Vec<u8>,
) -> RemoteChainHeadCallRequest {
	RemoteChainHeadCallRequest {
		genesis_hash: SHARED_GENESIS.to_vec(),
		follow_subscription_id: format!("c{instance}:follow"),
		hash: block.to_vec(),
		function: function.to_string(),
		call_parameters,
	}
}

fn output_of(follow: &mut FollowItems) -> Option<RemoteChainHeadFollowItem> {
	next_matching(follow, Duration::from_secs(5), is_call_end)
}

fn call_done(operation_id: &str) -> Option<RemoteChainHeadFollowItem> {
	Some(RemoteChainHeadFollowItem::OperationCallDone {
		operation_id: operation_id.to_string(),
		output: vec![1, 2],
	})
}

fn node_requests(provider: &ScriptedProvider, method: &str) -> usize {
	provider
		.sent
		.lock()
		.unwrap()
		.iter()
		.filter(|request| request.contains(method))
		.count()
}

/// The id of the first request for `method`, once the node has it.
fn request_sent(provider: &ScriptedProvider, method: &str) -> String {
	wait_for_sent(provider, |sent| sent.iter().any(|request| request.contains(method)))
		.iter()
		.find(|request| request.contains(method))
		.and_then(|request| extract_id(request))
		.expect("the request reaches the node")
}

/// Every product runtime holds its own connection, and each asks for the
/// metadata when it starts. One download serves them all, and a product that
/// asks once it has landed is answered from memory.
#[test]
fn products_share_one_metadata_download() {
	let provider = runtime_call_node(true);
	let core = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let (first, mut first_follow) = following_product(&core, 1);
	let (second, mut second_follow) = following_product(&core, 2);
	let (third, mut third_follow) = following_product(&core, 3);

	let first_operation = call(&first, 1, "Metadata_metadata_at_version", RUNTIME_BLOCK);
	let first_output = output_of(&mut first_follow);
	let second_operation = call(&second, 2, "Metadata_metadata_at_version", RUNTIME_BLOCK);
	let second_output = output_of(&mut second_follow);
	let third_operation = call(&third, 3, "Metadata_metadata_at_version", RUNTIME_BLOCK);
	let third_output = output_of(&mut third_follow);

	assert_eq!(
		(first_output, second_output, third_output, node_requests(&provider, "chainHead_v1_call")),
		(call_done(&first_operation), call_done(&second_operation), call_done(&third_operation), 1)
	);
}

/// A product that asks while the download is still on its way waits for it
/// instead of starting a second one over the same slow link.
#[test]
fn a_caller_during_the_download_waits_for_it() {
	let provider = runtime_call_node(false);
	let core = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let (first, mut first_follow) = following_product(&core, 1);
	let (second, mut second_follow) = following_product(&core, 2);

	let first_operation = call(&first, 1, "Metadata_metadata_at_version", RUNTIME_BLOCK);
	let second_operation = call(&second, 2, "Metadata_metadata_at_version", RUNTIME_BLOCK);
	notification_sender(&provider)
		.unbounded_send(call_done_event(&first_operation))
		.unwrap();

	assert_eq!(
		(
			output_of(&mut first_follow),
			output_of(&mut second_follow),
			node_requests(&provider, "chainHead_v1_call")
		),
		(call_done(&first_operation), call_done(&second_operation), 1)
	);
}

/// A runtime call that reads state can answer differently at every block,
/// so each product still asks the node.
#[test]
fn a_call_that_reads_state_is_never_shared() {
	let provider = runtime_call_node(true);
	let core = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let (first, mut first_follow) = following_product(&core, 1);
	let (second, mut second_follow) = following_product(&core, 2);

	let first_operation = call(&first, 1, "AccountNonceApi_account_nonce", RUNTIME_BLOCK);
	let first_output = output_of(&mut first_follow);
	let second_operation = call(&second, 2, "AccountNonceApi_account_nonce", RUNTIME_BLOCK);
	let second_output = output_of(&mut second_follow);

	assert_eq!(
		(first_output, second_output, node_requests(&provider, "chainHead_v1_call")),
		(call_done(&first_operation), call_done(&second_operation), 2)
	);
}

/// A chain-head client asks for the metadata at the first block its follow
/// reports, so products that started at different blocks of one runtime
/// still share the download.
#[test]
fn blocks_of_one_runtime_share_one_download() {
	let provider = runtime_call_node(true);
	let core = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let (first, mut first_follow) = following_product(&core, 1);
	let (second, mut second_follow) = following_product(&core, 2);

	let first_operation = call(&first, 1, "Metadata_metadata_at_version", RUNTIME_BLOCK);
	let first_output = output_of(&mut first_follow);
	let second_operation = call(&second, 2, "Metadata_metadata_at_version", LATER_BLOCK);
	let second_output = output_of(&mut second_follow);

	assert_eq!(
		(first_output, second_output, node_requests(&provider, "chainHead_v1_call")),
		(call_done(&first_operation), call_done(&second_operation), 1)
	);
}

/// The metadata of one runtime is not the metadata of another, so a block
/// after an upgrade asks the node again.
#[test]
fn blocks_of_different_runtimes_never_share() {
	let provider = runtime_call_node(true);
	let core = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let (first, mut first_follow) = following_product(&core, 1);
	let (second, mut second_follow) = following_product(&core, 2);

	let first_operation = call(&first, 1, "Metadata_metadata_at_version", RUNTIME_BLOCK);
	let first_output = output_of(&mut first_follow);
	let second_operation = call(&second, 2, "Metadata_metadata_at_version", UPGRADED_BLOCK);
	let second_output = output_of(&mut second_follow);

	assert_eq!(
		(first_output, second_output, node_requests(&provider, "chainHead_v1_call")),
		(call_done(&first_operation), call_done(&second_operation), 2)
	);
}

/// Without the hash of the runtime code at the block, nothing tells which
/// runtime answers the call, so it goes to the node.
#[test]
fn a_block_whose_code_cannot_be_read_is_never_shared() {
	let provider = runtime_call_node_with(true, |_| None);
	let core = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let (first, mut first_follow) = following_product(&core, 1);
	let (second, mut second_follow) = following_product(&core, 2);

	let first_operation = call(&first, 1, "Metadata_metadata_at_version", RUNTIME_BLOCK);
	let first_output = output_of(&mut first_follow);
	let second_operation = call(&second, 2, "Metadata_metadata_at_version", RUNTIME_BLOCK);
	let second_output = output_of(&mut second_follow);

	assert_eq!(
		(first_output, second_output, node_requests(&provider, "chainHead_v1_call")),
		(call_done(&first_operation), call_done(&second_operation), 2)
	);
}

/// When the product whose call is the download stops it, the products
/// waiting on it are told to retry instead of waiting for an answer that
/// never comes.
#[test]
fn stopping_the_download_releases_its_waiters() {
	let provider = runtime_call_node(false);
	let core = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let (first, _first_follow) = following_product(&core, 1);
	let (second, mut second_follow) = following_product(&core, 2);

	let first_operation = call(&first, 1, "Metadata_metadata_at_version", RUNTIME_BLOCK);
	let second_operation = call(&second, 2, "Metadata_metadata_at_version", RUNTIME_BLOCK);
	futures::executor::block_on(first.remote_chain_head_stop_operation(
		RemoteChainHeadStopOperationRequest {
			genesis_hash: SHARED_GENESIS.to_vec(),
			follow_subscription_id: "c1:follow".to_string(),
			operation_id: first_operation,
		},
	))
	.expect("the stop goes through");

	assert_eq!(
		output_of(&mut second_follow),
		Some(RemoteChainHeadFollowItem::OperationInaccessible { operation_id: second_operation })
	);
}

/// The node reports the end of an operation on the follow it was made on,
/// so once that follow ends the download made on it never lands. The
/// products waiting on it are told to retry.
#[test]
fn a_follow_ending_during_the_download_releases_its_waiters() {
	let provider = runtime_call_node(false);
	let core = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let (first, first_follow) = following_product(&core, 1);
	let (second, mut second_follow) = following_product(&core, 2);
	call(&first, 1, "Metadata_metadata_at_version", RUNTIME_BLOCK);
	let second_operation = call(&second, 2, "Metadata_metadata_at_version", RUNTIME_BLOCK);

	drop(first_follow);

	assert_eq!(
		output_of(&mut second_follow),
		Some(RemoteChainHeadFollowItem::OperationInaccessible { operation_id: second_operation })
	);
}

/// A follow can end while its download still waits for the reply that
/// names the operation. That download never lands either, so the products
/// waiting on it are told to retry.
#[test]
fn a_follow_ending_before_the_node_names_the_download_releases_its_waiters() {
	let provider = runtime_call_node_withholding(&["chainHead_v1_call"]);
	let core = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let (first, first_follow) = following_product(&core, 1);
	let (second, mut second_follow) = following_product(&core, 2);
	std::thread::spawn(move || call(&first, 1, "Metadata_metadata_at_version", RUNTIME_BLOCK));
	let download = request_sent(&provider, "chainHead_v1_call");
	let second_operation = call(&second, 2, "Metadata_metadata_at_version", RUNTIME_BLOCK);

	drop(first_follow);
	request_sent(&provider, "chainHead_v1_unfollow");
	notification_sender(&provider)
		.unbounded_send(format!(
			r#"{{"jsonrpc":"2.0","id":"{download}","result":{{"result":"started","operationId":"call-1"}}}}"#
		))
		.unwrap();

	assert_eq!(
		output_of(&mut second_follow),
		Some(RemoteChainHeadFollowItem::OperationInaccessible { operation_id: second_operation })
	);
}

/// Closing a product drops the calls it has in flight, which can be while its
/// download still waits for the reply that names the operation. That download
/// never lands either, so the products waiting on it are told to retry.
#[test]
fn a_call_dropped_before_the_node_names_the_download_releases_its_waiters() {
	let provider = runtime_call_node_withholding(&["chainHead_v1_call"]);
	let core = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let (first, _first_follow) = following_product(&core, 1);
	let (second, mut second_follow) = following_product(&core, 2);
	let (abort, registration) = AbortHandle::new_pair();
	std::thread::spawn(move || {
		let request =
			call_request(1, "Metadata_metadata_at_version", RUNTIME_BLOCK, vec![15, 0, 0, 0]);
		futures::executor::block_on(Abortable::new(
			first.remote_chain_head_call(request),
			registration,
		))
	});
	request_sent(&provider, "chainHead_v1_call");
	let second_operation = call(&second, 2, "Metadata_metadata_at_version", RUNTIME_BLOCK);

	abort.abort();

	assert_eq!(
		output_of(&mut second_follow),
		Some(RemoteChainHeadFollowItem::OperationInaccessible { operation_id: second_operation })
	);
}

/// A follow can also end while a call on it reads the hash of the runtime
/// code. That call reaches the node as no download at all, so a product
/// that asks next downloads the metadata itself instead of waiting.
#[test]
fn a_call_on_a_follow_that_ended_is_never_waited_on() {
	let provider = runtime_call_node_withholding(&["state_getStorageHash", "chainHead_v1_call"]);
	let core = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let (first, first_follow) = following_product(&core, 1);
	let (second, mut second_follow) = following_product(&core, 2);
	std::thread::spawn(move || call(&first, 1, "Metadata_metadata_at_version", RUNTIME_BLOCK));
	let code_hash_read = request_sent(&provider, "state_getStorageHash");
	drop(first_follow);
	request_sent(&provider, "chainHead_v1_unfollow");
	notification_sender(&provider)
		.unbounded_send(format!(
			r#"{{"jsonrpc":"2.0","id":"{code_hash_read}","result":"0x{}"}}"#,
			hex::encode([0xc1; 32])
		))
		.unwrap();
	request_sent(&provider, "chainHead_v1_call");

	let second_operation = call(&second, 2, "Metadata_metadata_at_version", RUNTIME_BLOCK);

	assert_eq!(
		(output_of(&mut second_follow), node_requests(&provider, "chainHead_v1_call")),
		(call_done(&second_operation), 2)
	);
}

/// A product that stops an operation the cache answers for has nothing to
/// stop at the node, and stops getting the answer.
#[test]
fn stopping_a_cached_operation_never_reaches_the_node() {
	let provider = runtime_call_node(false);
	let core = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let (first, mut first_follow) = following_product(&core, 1);
	let (second, mut second_follow) = following_product(&core, 2);

	let first_operation = call(&first, 1, "Metadata_metadata_at_version", RUNTIME_BLOCK);
	let second_operation = call(&second, 2, "Metadata_metadata_at_version", RUNTIME_BLOCK);
	futures::executor::block_on(second.remote_chain_head_stop_operation(
		RemoteChainHeadStopOperationRequest {
			genesis_hash: SHARED_GENESIS.to_vec(),
			follow_subscription_id: "c2:follow".to_string(),
			operation_id: second_operation,
		},
	))
	.expect("the stop goes through");
	notification_sender(&provider)
		.unbounded_send(call_done_event(&first_operation))
		.unwrap();

	assert_eq!(
		(
			output_of(&mut first_follow),
			next_matching(&mut second_follow, Duration::from_millis(300), is_call_end),
			node_requests(&provider, "chainHead_v1_stopOperation")
		),
		(call_done(&first_operation), None, 0)
	);
}

/// A node that serves the bulletin fixture runtime over chainHead, to a
/// Subxt client as much as to products, with every follow starting at
/// `RUNTIME_BLOCK` under a subscription id of its own.
fn subxt_node() -> Arc<ScriptedProvider> {
	subxt_node_with(|_, _| true)
}

/// As [`subxt_node`], where the m-th call the node gets starts as
/// `call-<m>`, and a call of `function` on the n-th follow finishes only
/// when `answers(n, function)` holds.
fn subxt_node_with(answers: fn(usize, &str) -> bool) -> Arc<ScriptedProvider> {
	let follows = Arc::new(AtomicUsize::new(0));
	let calls = Arc::new(AtomicUsize::new(0));
	Arc::new(ScriptedProvider::with_frames(move |request| {
		let id = extract_id(request).unwrap();
		let request: Value = serde_json::from_str(request).unwrap();
		let reply = |result: Value| {
			serde_json::json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string()
		};
		let params = &request["params"];
		match request["method"].as_str() {
			Some("chainHead_v1_follow") => {
				let follow = format!("follow-{}", follows.fetch_add(1, Ordering::SeqCst) + 1);
				vec![
					reply(serde_json::json!(follow)),
					follow_event_on(
						&follow,
						serde_json::json!({
							"event": "initialized",
							"finalizedBlockHashes": [format!("0x{}", hex::encode(RUNTIME_BLOCK))],
							"finalizedBlockRuntime": {"type": "valid", "spec": {
								"specName": "bulletin",
								"implName": "bulletin",
								"specVersion": 1,
								"implVersion": 1,
								"transactionVersion": 1,
								"apis": {},
							}},
						}),
					),
				]
			},
			Some("chainHead_v1_header") => {
				let header = subxt::config::substrate::SubstrateHeader::<H256> {
					parent_hash: H256::zero(),
					number: 1,
					state_root: H256::zero(),
					extrinsics_root: H256::zero(),
					digest: Default::default(),
				};
				vec![reply(serde_json::json!(format!("0x{}", hex::encode(header.encode()))))]
			},
			Some("chainHead_v1_call") => {
				let follow = params[0].as_str().unwrap();
				let follow_number: usize = follow.trim_start_matches("follow-").parse().unwrap();
				let function = params[2].as_str().unwrap();
				let operation = format!("call-{}", calls.fetch_add(1, Ordering::SeqCst) + 1);
				let started =
					reply(serde_json::json!({"result": "started", "operationId": operation}));
				if !answers(follow_number, function) {
					return vec![started];
				}
				vec![started, subxt_call_done(follow, &operation, function)]
			},
			Some("state_getStorageHash") => {
				vec![reply(serde_json::json!(format!("0x{}", hex::encode([0xc1; 32]))))]
			},
			Some("chainHead_v1_unpin" | "chainHead_v1_stopOperation") => {
				vec![reply(Value::Null)]
			},
			_ => Vec::new(),
		}
	}))
}

/// The event reporting the call `operation` of `function` on `follow` done,
/// with the output the bulletin fixture gives.
fn subxt_call_done(follow: &str, operation: &str, function: &str) -> String {
	let output = crate::host_internal::extrinsic::tests::bulletin_runtime_call(function)
		.expect("the fixture answers every call Subxt makes to load the runtime");
	follow_event_on(
		follow,
		serde_json::json!({
			"event": "operationCallDone",
			"operationId": operation,
			"output": format!("0x{}", hex::encode(output)),
		}),
	)
}

/// Whether `client` loads the runtime at the current block within five
/// seconds, so a load that never finishes fails one test instead of hanging
/// the rest.
fn runtime_loads(client: &OnlineClient<SubstrateConfig>) -> bool {
	futures::executor::block_on(async {
		let load = client.at_current_block().fuse();
		let deadline = futures_timer::Delay::new(Duration::from_secs(5)).fuse();
		pin_mut!(load, deadline);
		futures::select! {
			loaded = load => loaded.is_ok(),
			() = deadline => false,
		}
	})
}

/// The core builds transactions with a Subxt client on a connection of its
/// own. Once a product has downloaded the metadata, that client loads the
/// runtime from memory instead of downloading it again over the same link.
#[test]
fn the_core_reuses_metadata_a_product_downloaded() {
	let provider = subxt_node();
	let core = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let (product, mut product_follow) = following_product(&core, 1);
	call_with(&product, 1, "Metadata_metadata_at_version", RUNTIME_BLOCK, 14u32.encode());
	output_of(&mut product_follow).expect("the product gets the metadata");

	let client = futures::executor::block_on(core.online_client(&SHARED_GENESIS)).expect("client");
	let loaded = runtime_loads(&client);

	assert_eq!((loaded, node_requests(&provider, "Metadata_metadata_at_version")), (true, 1));
}

/// Subxt never retries a call the node reports inaccessible, so the client
/// of the core must not wait on a download a product may give up. It
/// downloads on its own instead.
#[test]
fn the_core_never_waits_on_a_download_in_flight() {
	let provider = subxt_node_with(|follow, _| follow != 1);
	let core = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let (product, _product_follow) = following_product(&core, 1);
	call_with(&product, 1, "Metadata_metadata_at_version", RUNTIME_BLOCK, 14u32.encode());

	let client = futures::executor::block_on(core.online_client(&SHARED_GENESIS)).expect("client");
	let loaded = runtime_loads(&client);

	assert_eq!((loaded, node_requests(&provider, "Metadata_metadata_at_version")), (true, 2));
}

/// When the download a product owns stalls, the core downloads alongside
/// it, and the first output to land answers the products waiting.
#[test]
fn a_download_alongside_a_stalled_one_answers_its_waiters() {
	let provider = subxt_node_with(|follow, _| follow != 1);
	let core = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let (first, _first_follow) = following_product(&core, 1);
	let (second, mut second_follow) = following_product(&core, 2);
	call_with(&first, 1, "Metadata_metadata_at_version", RUNTIME_BLOCK, 14u32.encode());
	let waiting =
		call_with(&second, 2, "Metadata_metadata_at_version", RUNTIME_BLOCK, 14u32.encode());

	let client = futures::executor::block_on(core.online_client(&SHARED_GENESIS)).expect("client");
	let loaded = runtime_loads(&client);

	assert_eq!(
		(
			loaded,
			is_call_end_for(&output_of(&mut second_follow), &waiting),
			node_requests(&provider, "Metadata_metadata_at_version")
		),
		(true, true, 2)
	);
}

/// The core loads the runtime when it first builds a transaction, which can
/// be while the products start. A product that asks during that download
/// waits for it instead of starting a second one over the same link.
#[test]
fn a_product_during_the_core_download_waits_for_it() {
	let provider = subxt_node_with(|follow, function| {
		follow != 1 || function != "Metadata_metadata_at_version"
	});
	let core = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let client = futures::executor::block_on(core.online_client(&SHARED_GENESIS)).expect("client");
	std::thread::spawn(move || futures::executor::block_on(client.at_current_block()).is_ok());
	request_sent(&provider, "Metadata_metadata_at_version");
	let core_download = format!("call-{}", node_requests(&provider, "chainHead_v1_call"));
	let (product, mut product_follow) = following_product(&core, 1);

	let operation =
		call_with(&product, 1, "Metadata_metadata_at_version", RUNTIME_BLOCK, 14u32.encode());
	let downloads = node_requests(&provider, "Metadata_metadata_at_version");
	notification_sender(&provider)
		.unbounded_send(subxt_call_done("follow-1", &core_download, "Metadata_metadata_at_version"))
		.unwrap();

	assert_eq!(
		(downloads, is_call_end_for(&output_of(&mut product_follow), &operation)),
		(1, true)
	);
}

/// A node may report an operation done before the reply that names it. The
/// download still counts, and the product waiting on it gets the output.
#[test]
fn a_download_reported_before_its_reply_still_answers_its_waiters() {
	let provider = runtime_call_node_withholding(&["chainHead_v1_call"]);
	let core = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let (first, mut first_follow) = following_product(&core, 1);
	let (second, mut second_follow) = following_product(&core, 2);
	std::thread::spawn(move || call(&first, 1, "Metadata_metadata_at_version", RUNTIME_BLOCK));
	let download = request_sent(&provider, "chainHead_v1_call");
	let second_operation = call(&second, 2, "Metadata_metadata_at_version", RUNTIME_BLOCK);
	let node = notification_sender(&provider);

	node.unbounded_send(call_done_event("call-1")).unwrap();
	let reported = output_of(&mut first_follow);
	node.unbounded_send(format!(
		r#"{{"jsonrpc":"2.0","id":"{download}","result":{{"result":"started","operationId":"call-1"}}}}"#
	))
	.unwrap();

	assert_eq!(
		(reported, output_of(&mut second_follow), node_requests(&provider, "chainHead_v1_call")),
		(call_done("call-1"), call_done(&second_operation), 1)
	);
}

/// When the core loads a runtime first, its download serves the products
/// that ask for the same metadata afterwards.
#[test]
fn products_reuse_metadata_the_core_downloaded() {
	let provider = subxt_node();
	let core = ChainRuntime::new(provider.clone(), spawner_for_tests());
	let client = futures::executor::block_on(core.online_client(&SHARED_GENESIS)).expect("client");
	let loaded = runtime_loads(&client);
	let (product, mut product_follow) = following_product(&core, 1);

	let operation =
		call_with(&product, 1, "Metadata_metadata_at_version", RUNTIME_BLOCK, 14u32.encode());

	assert_eq!(
		(
			loaded,
			is_call_end_for(&output_of(&mut product_follow), &operation),
			node_requests(&provider, "Metadata_metadata_at_version")
		),
		(true, true, 1)
	);
}

fn is_call_end_for(item: &Option<RemoteChainHeadFollowItem>, operation: &str) -> bool {
	matches!(
		item,
		Some(RemoteChainHeadFollowItem::OperationCallDone { operation_id, .. }) if operation_id == operation
	)
}

/// The budget line tells a follow nobody ended from one in use: how long it
/// has been open, whether the node still sends it events, how recently a call
/// was made on it, and whether its receiver is still open.
#[test]
fn a_follow_holding_a_slot_says_how_long_it_has_been_open_and_when_it_was_last_used() {
	let opened = SystemTime::now();
	let at = |seconds| opened + Duration::from_secs(seconds);
	let (sender, receiver) = mpsc::unbounded();
	let mut follow = FollowState {
		with_runtime: false,
		client_subscription_id: None,
		remote_subscription_id: Some("REMOTE".to_string()),
		abort: None,
		sender,
		cancelled: Arc::new(AtomicBool::new(false)),
		activity: FollowActivity {
			opened_at: opened,
			last_event_at: None,
			calls: 0,
			last_call_at: None,
		},
	};
	assert_eq!(
		follow.describe_activity(at(600)),
		"open 600 s, last event never, 0 calls, last call never, receiver open"
	);

	follow.activity.last_event_at = Some(at(594));
	follow.activity.calls = 3;
	follow.activity.last_call_at = Some(at(60));
	drop(receiver);
	assert_eq!(
		follow.describe_activity(at(600)),
		"open 600 s, last event 6 s ago, 3 calls, last call 540 s ago, receiver dropped"
	);
}
