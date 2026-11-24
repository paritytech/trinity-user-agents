use super::*;

#[test]
#[should_panic(
	expected = "`state_getStorage` was called 2 times, and the script has 1 response(s) for it"
)]
fn method_keyed_responses_do_not_replay_an_exhausted_answer() {
	use futures::StreamExt;

	let request =
		|id| format!(r#"{{"jsonrpc":"2.0","id":"{id}","method":"state_getStorage","params":[]}}"#);
	let sent = Arc::new(Mutex::new(vec![request(1), request(2)]));
	let mut responses =
		method_keyed_responses(sent, vec![("state_getStorage", "null".to_string())]);

	futures::executor::block_on(async {
		responses.next().await.expect("first scripted response");
		responses.next().await.expect("second scripted response");
	});
}

fn assert_rpc_responses_are_connection_local(platform: StubPlatform) {
	use futures::FutureExt;

	futures::executor::block_on(async {
		let first = platform.connect([0xaa; 32]).await.unwrap();
		let second = platform.connect([0xaa; 32]).await.unwrap();
		let mut first_responses = first.responses();
		let mut second_responses = second.responses();
		let mut expected_requests = Vec::new();

		for index in 1..=2 {
			let request = format!(
				r#"{{"jsonrpc":"2.0","id":"truapi:{index}","method":"state_getStorage","params":[]}}"#
			);
			let response =
				format!(r#"{{"jsonrpc":"2.0","id":"truapi:{index}","result":"0x0{index}"}}"#);
			first.send(request.clone());
			assert_eq!(first_responses.next().await, Some(response.clone()));
			assert_eq!(second_responses.next().now_or_never(), None);

			second.send(request.clone());
			assert_eq!(second_responses.next().await, Some(response));
			expected_requests.extend([request.clone(), request]);
		}

		assert_eq!(*platform.sent_rpc.lock().unwrap(), expected_requests);
	});
}

#[test]
fn scripted_rpc_responses_wait_for_their_own_connection_request() {
	assert_rpc_responses_are_connection_local(StubPlatform {
		rpc_responses: vec![
			r#"{"jsonrpc":"2.0","id":"truapi:1","result":"0x01"}"#.to_string(),
			r#"{"jsonrpc":"2.0","id":"truapi:2","result":"0x02"}"#.to_string(),
		],
		..Default::default()
	});
}

#[test]
fn method_keyed_rpc_responses_count_requests_per_connection() {
	assert_rpc_responses_are_connection_local(StubPlatform {
		rpc_method_responses: vec![
			("state_getStorage", r#""0x01""#.to_string()),
			("state_getStorage", r#""0x02""#.to_string()),
		],
		..Default::default()
	});
}
