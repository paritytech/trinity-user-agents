//! Browser bootstrap shared by the native hosts.

/// Render the endpoint configuration consumed by the host container.
pub fn script(url: &str) -> String {
	let url = serde_json::to_string(url).expect("a string always serializes");
	format!("window.__truapi_localhost = {{ url: {url} }};")
}

#[cfg(test)]
mod tests {
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
}
