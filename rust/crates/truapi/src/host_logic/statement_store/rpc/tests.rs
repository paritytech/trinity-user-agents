use super::*;

#[test]
fn parses_dotli_sdk_new_statements_result() {
	let result = serde_json::json!({
		"event": "newStatements",
		"data": {
			"statements": ["0xdeadbeef", "0xcafe"],
			"remaining": 0,
		},
	});

	assert_eq!(
		parse_new_statements_result("remote-sub".to_string(), &result).unwrap(),
		NewStatements {
			remote_subscription_id: "remote-sub".to_string(),
			statements: vec![vec![0xde, 0xad, 0xbe, 0xef], vec![0xca, 0xfe]],
			remaining: Some(0),
		}
	);
}
