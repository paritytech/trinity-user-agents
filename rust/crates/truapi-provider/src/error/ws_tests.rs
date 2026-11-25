use super::redacted;

#[test]
fn userinfo_is_stripped_from_a_node_url() {
	let url = url::Url::parse("wss://alice:hunter2@node.example/path").expect("parse");

	let printed = redacted(&url);

	assert!(!printed.contains("alice"), "leaked the username: {printed}");
	assert!(!printed.contains("hunter2"), "leaked the password: {printed}");
	assert!(printed.contains("node.example"), "lost the host: {printed}");
}

#[test]
fn a_username_without_a_password_is_stripped_too() {
	let url = url::Url::parse("wss://token@node.example/").expect("parse");

	assert!(!redacted(&url).contains("token"));
}

/// A URL with no userinfo is the common case and must survive verbatim, so
/// the diagnostic keeps its path and port.
#[test]
fn a_url_without_credentials_is_unchanged() {
	let url = url::Url::parse("wss://node.example:9944/ws").expect("parse");

	assert_eq!(redacted(&url), url.to_string());
}
