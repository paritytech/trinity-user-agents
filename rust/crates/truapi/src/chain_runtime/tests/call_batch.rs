use super::*;
use crate::time::ManualTimer;

fn call_batch_fixture(
	functions: &[&str],
) -> (ChainRuntime, BoxStream<'static, RemoteChainHeadFollowItem>, Vec<RemoteChainHeadCallRequest>)
{
	let respond = runtime_call_frames(false, code_hash_with_upgrade);
	let provider = Arc::new(ScriptedProvider::with_frames(move |request| {
		let value: Value = serde_json::from_str(request).unwrap();
		if value["method"] != "chainHead_v1_call" {
			return respond(request);
		}
		let function = value["params"][2].as_str().unwrap();
		let result = if function == "limited" {
			serde_json::json!({"result": "limitReached"})
		} else {
			serde_json::json!({"result": "started", "operationId": function})
		};
		vec![serde_json::json!({"jsonrpc": "2.0", "id": value["id"], "result": result}).to_string()]
	}));
	let core = ChainRuntime::new(provider, Spawner::for_test());
	let (chain, follow) = following_product(&core, 1);
	let requests = functions
		.iter()
		.map(|function| call_request(1, function, RUNTIME_BLOCK, Vec::new()))
		.collect();
	(chain, follow.map(Result::unwrap).boxed(), requests)
}

#[test]
fn a_call_batch_keeps_early_results_in_request_order() {
	let (chain, follow, requests) = call_batch_fixture(&["first", "second"]);
	let mut follow = stream::iter([
		RemoteChainHeadFollowItem::OperationCallDone {
			operation_id: "second".into(),
			output: vec![2],
		},
		RemoteChainHeadFollowItem::OperationCallDone {
			operation_id: "unrelated".into(),
			output: vec![0],
		},
		RemoteChainHeadFollowItem::OperationCallDone {
			operation_id: "first".into(),
			output: vec![1],
		},
	])
	.chain(follow)
	.boxed();

	assert_eq!(
		futures::executor::block_on(chain.chain_head_call_batch(
			&mut follow,
			requests,
			"Asset Hub",
			Duration::from_secs(10),
			&ManualTimer::default(),
		)),
		vec![Ok(vec![1]), Ok(vec![2])]
	);
}

#[test]
fn a_call_batch_keeps_each_operations_failure() {
	let (chain, follow, mut requests) =
		call_batch_fixture(&["limited", "invalid", "failed", "gone", "first"]);
	requests[1].hash = vec![0];
	let mut follow = stream::iter([
		RemoteChainHeadFollowItem::OperationInaccessible { operation_id: "gone".into() },
		RemoteChainHeadFollowItem::OperationError {
			operation_id: "failed".into(),
			error: "execution failed".into(),
		},
		RemoteChainHeadFollowItem::OperationCallDone {
			operation_id: "first".into(),
			output: vec![1],
		},
	])
	.chain(follow)
	.boxed();

	assert_eq!(
		futures::executor::block_on(chain.chain_head_call_batch(
			&mut follow,
			requests,
			"Asset Hub",
			Duration::from_secs(10),
			&ManualTimer::default(),
		)),
		vec![
			Err("Asset Hub operation limit reached".into()),
			Err("remote_chain_head_call: expected 32-byte hash, got 1".into()),
			Err("execution failed".into()),
			Err("Asset Hub runtime call was inaccessible".into()),
			Ok(vec![1]),
		]
	);
}

#[test]
fn a_stopped_call_batch_preserves_completed_results() {
	for ending in [vec![RemoteChainHeadFollowItem::Stop], vec![]] {
		let (chain, follow, requests) = call_batch_fixture(&["first", "second"]);
		let event_count = 1 + ending.len();
		let mut follow = stream::iter([RemoteChainHeadFollowItem::OperationCallDone {
			operation_id: "first".into(),
			output: vec![1],
		}])
		.chain(stream::iter(ending))
		.chain(follow)
		.take(event_count)
		.boxed();

		assert_eq!(
			futures::executor::block_on(chain.chain_head_call_batch(
				&mut follow,
				requests,
				"Asset Hub",
				Duration::from_secs(10),
				&ManualTimer::default(),
			)),
			vec![Ok(vec![1]), Err("Asset Hub follow stopped during runtime call".into())]
		);
	}
}

#[test]
fn a_call_batch_times_out_only_unfinished_operations() {
	let (chain, follow, requests) = call_batch_fixture(&["first", "second"]);
	futures::executor::block_on(async {
		let timer = ManualTimer::default();
		let mut follow = stream::iter([RemoteChainHeadFollowItem::OperationCallDone {
			operation_id: "first".into(),
			output: vec![1],
		}])
		.chain(follow)
		.boxed();
		let batch = chain.chain_head_call_batch(
			&mut follow,
			requests,
			"Asset Hub",
			Duration::from_secs(10),
			&timer,
		);
		pin_mut!(batch);
		futures::future::poll_fn(|context| {
			assert!(batch.as_mut().poll(context).is_pending());
			if timer.sleeps() == 1 { Poll::Ready(()) } else { Poll::Pending }
		})
		.await;
		timer.advance(Duration::from_secs(9));
		assert!(futures::poll!(&mut batch).is_pending());
		timer.advance(Duration::from_secs(1));

		assert_eq!(batch.await, vec![Ok(vec![1]), Err("Asset Hub runtime call timed out".into())]);
	});
}

#[test]
fn a_call_batch_with_no_started_operations_does_not_read_the_follow() {
	for functions in [vec![], vec!["limited", "limited"]] {
		let (chain, follow, requests) = call_batch_fixture(&functions);
		let mut follow = stream::poll_fn(move |_| {
			let _ = &follow;
			panic!("no operation started, so no follow event is needed")
		})
		.boxed();

		assert_eq!(
			futures::executor::block_on(chain.chain_head_call_batch(
				&mut follow,
				requests,
				"Asset Hub",
				Duration::from_secs(10),
				&ManualTimer::default(),
			)),
			vec![Err("Asset Hub operation limit reached".into()); functions.len()]
		);
	}
}
