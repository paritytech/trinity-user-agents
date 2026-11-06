use super::*;

#[test]
fn script_carries_the_endpoint() {
	let script = script("ws://127.0.0.1:9955/?t=abc");

	assert_eq!(script, r#"window.__truapi_localhost = { url: "ws://127.0.0.1:9955/?t=abc" };"#);
}

#[test]
fn script_escapes_an_endpoint_that_would_otherwise_break_out() {
	let script = script(r#"ws://x";alert(1);//"#);

	assert_eq!(script, r#"window.__truapi_localhost = { url: "ws://x\";alert(1);//" };"#);
}
