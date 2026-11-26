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

/// Providers put API keys in the path or under query names of their own, so
/// neither survives, while the host and port still name the node.
#[test]
fn the_path_and_query_are_dropped() {
	let url = url::Url::parse("wss://node.example:9944/v1/PATHKEY?apikey=SECRET").expect("parse");

	assert_eq!(redacted(&url), "wss://node.example:9944");
}
