use std::{collections::HashMap, sync::Mutex};

use futures::{channel::oneshot, executor::block_on};
use serde_json::{Value, json};
use subxt_rpcs::client::{RawRpcFuture, RawRpcSubscription, RawValue, RpcClientT};

use super::*;

struct PendingRpc {
	requests: Mutex<Vec<Value>>,
	replies: Mutex<HashMap<String, oneshot::Receiver<Result<Value, subxt_rpcs::Error>>>>,
}

impl RpcClientT for PendingRpc {
	fn request_raw<'a>(
		&'a self,
		method: &'a str,
		params: Option<Box<RawValue>>,
	) -> RawRpcFuture<'a, Box<RawValue>> {
		assert_eq!(method, "state_call");
		let params: Value = serde_json::from_str(params.unwrap().get()).unwrap();
		let reply = self.replies.lock().unwrap().remove(params[1].as_str().unwrap()).unwrap();
		self.requests.lock().unwrap().push(params);
		Box::pin(async move {
			let value = reply.await.expect("the test provides a reply")?;
			Ok(RawValue::from_string(value.to_string()).unwrap())
		})
	}

	fn subscribe_raw<'a>(
		&'a self,
		_sub: &'a str,
		_params: Option<Box<RawValue>>,
		_unsub: &'a str,
	) -> RawRpcFuture<'a, RawRpcSubscription> {
		unreachable!("runtime API calls do not subscribe")
	}
}

#[test]
fn rpc_batches_start_together_and_keep_out_of_order_successes_and_errors_in_input_order() {
	block_on(async {
		let (first_reply, first_response) = oneshot::channel();
		let (second_reply, second_response) = oneshot::channel();
		let (third_reply, third_response) = oneshot::channel();
		let transport = std::sync::Arc::new(PendingRpc {
			requests: Mutex::new(Vec::new()),
			replies: Mutex::new(HashMap::from([
				("0x01".to_string(), first_response),
				("0x02".to_string(), second_response),
				("0x03".to_string(), third_response),
			])),
		});
		let mut rpc = RpcClient::new(transport.clone());
		let mut batch =
			core::pin::pin!(rpc.call_batch("Read_api", vec![vec![1], vec![2], vec![3]]));

		assert!(futures::poll!(&mut batch).is_pending());
		let mut requests = transport.requests.lock().unwrap().clone();
		requests.sort_by_key(|request| request[1].as_str().unwrap().to_owned());
		assert_eq!(
			requests,
			vec![
				json!(["Read_api", "0x01", null]),
				json!(["Read_api", "0x02", null]),
				json!(["Read_api", "0x03", null]),
			]
		);

		third_reply.send(Ok(json!("0x33"))).unwrap();
		assert!(futures::poll!(&mut batch).is_pending());
		second_reply
			.send(Err(subxt_rpcs::UserError {
				code: -32603,
				message: "runtime unavailable".to_string(),
				data: Some(RawValue::from_string(r#"{"retry":true}"#.to_string()).unwrap()),
			}
			.into()))
			.unwrap();
		assert!(futures::poll!(&mut batch).is_pending());
		first_reply.send(Ok(json!("0x11"))).unwrap();

		let outcomes: Vec<_> = batch
			.await
			.into_iter()
			.map(|result| {
				result.map_err(|error| match error {
					subxt::Error::OtherRpcClientError(subxt_rpcs::Error::User(error)) => {
						(error.code, error.message, error.data.map(|data| data.get().to_string()))
					},
					other => panic!("RPC error was not preserved: {other:?}"),
				})
			})
			.collect();
		assert_eq!(
			outcomes,
			vec![
				Ok(vec![0x11]),
				Err((
					-32603,
					"runtime unavailable".to_string(),
					Some(r#"{"retry":true}"#.to_string())
				)),
				Ok(vec![0x33]),
			]
		);
	});
}
