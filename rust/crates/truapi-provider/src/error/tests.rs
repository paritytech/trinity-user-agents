use super::{FrameForId, frame_for_id, synthetic_error_frame};

#[test]
fn a_matching_result_frame_yields_its_string() {
	let frame = frame_for_id(r#"{"id":"db","result":"blob"}"#, "db").expect("the id matches");
	let FrameForId::Result(result) = frame else {
		panic!("expected a result");
	};
	assert_eq!(result, "blob");
}

#[test]
fn a_matching_error_frame_fails_instead_of_waiting() {
	// A node that does not implement the method answers with an error and
	// keeps the socket open, so this must terminate the caller's wait.
	let frame =
		frame_for_id(r#"{"id":"db","error":{"code":-32601,"message":"Method not found"}}"#, "db")
			.expect("the id matches");
	let FrameForId::Failure(reason) = frame else {
		panic!("expected a failure");
	};
	assert_eq!(reason, "Method not found");
}

#[test]
fn a_matching_non_string_result_fails() {
	let frame = frame_for_id(r#"{"id":"db","result":null}"#, "db").expect("the id matches");
	assert!(matches!(frame, FrameForId::Failure(_)));
}

#[test]
fn other_ids_and_notifications_are_not_matched() {
	assert!(frame_for_id(r#"{"id":"other","result":"blob"}"#, "db").is_none());
	assert!(frame_for_id(r#"{"method":"chainHead_v1_followEvent"}"#, "db").is_none());
	assert!(frame_for_id("not json", "db").is_none());
}

#[test]
fn echoes_the_request_id() {
	let frame =
		synthetic_error_frame(r#"{"jsonrpc":"2.0","id":7,"method":"x","params":[]}"#, "queue full")
			.expect("a request with an id yields an error frame");
	let value: serde_json::Value = serde_json::from_str(&frame).expect("valid JSON");
	assert_eq!(value["id"], 7);
	assert_eq!(value["error"]["code"], -32603);
	assert_eq!(value["error"]["message"], "queue full");
}

#[test]
fn string_ids_are_preserved() {
	let frame = synthetic_error_frame(r#"{"id":"abc","method":"x"}"#, "boom").expect("has an id");
	let value: serde_json::Value = serde_json::from_str(&frame).expect("valid JSON");
	assert_eq!(value["id"], "abc");
}

#[test]
fn notifications_and_garbage_yield_nothing() {
	assert!(synthetic_error_frame(r#"{"method":"x","params":[]}"#, "m").is_none());
	assert!(synthetic_error_frame(r#"{"id":null,"method":"x"}"#, "m").is_none());
	assert!(synthetic_error_frame("not json", "m").is_none());
}
