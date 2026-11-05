use trinity_stack::DEFAULT_LINK_BASE;

use super::*;

#[test]
fn a_key_that_contradicts_the_link_is_refused() {
	let key = format!("0x{}", hex::encode(trinity_stack::public_key(&[7; 32])));
	let other = format!("0x{}", hex::encode(trinity_stack::public_key(&[9; 32])));
	let link = Pointer::new("https://a.example/s.json", Some(&key))
		.unwrap()
		.to_link(DEFAULT_LINK_BASE);
	assert!(pointer_of(&link, Some(&other)).is_err());
	assert_eq!(
		pointer_of(&link, Some(&key)).unwrap().unwrap().key(),
		Pointer::from_link(&link).unwrap().key()
	);
	let unkeyed = Pointer::new("https://a.example/s.json", None)
		.unwrap()
		.to_link(DEFAULT_LINK_BASE);
	let filled = pointer_of(&unkeyed, Some(&key)).unwrap().unwrap();
	assert_eq!(filled.key().map(hex::encode), Some(key[2..].to_owned()));
	assert!(pointer_of("./stack.json", None).unwrap().is_none());
}

#[test]
fn a_link_or_url_breaking_a_rule_says_which_instead_of_reading_a_file() {
	let code = |stack: &str| pointer_of(stack, None).unwrap_err().to_string();
	let plain_http = "polkadotapp://stack?url=http%3A%2F%2Fstack.example%2Fs.json";
	assert!(code(plain_http).contains("invalid-url"), "{}", code(plain_http));
	let short_key = "polkadotapp://stack?key=0x12&url=https%3A%2F%2Fstack.example%2Fs.json";
	assert!(code(short_key).contains("invalid-hex"), "{}", code(short_key));
	assert!(code("http://stack.example/s.json").contains("invalid-url"));
	let app_link = "https://app.example/open?url=https%3A%2F%2Fstack.example%2Fs.json";
	assert_eq!(
		pointer_of(app_link, None).unwrap().unwrap().url().as_str(),
		"https://stack.example/s.json"
	);
}

/// A server that answers with an error status is told apart from one that
/// does not answer, so a moved descriptor can be reported louder.
#[tokio::test]
async fn test_fetch_http_status_error() {
	use tokio::io::{AsyncReadExt, AsyncWriteExt};

	let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
	let url = format!("http://{}/stack.json", listener.local_addr().unwrap());
	tokio::spawn(async move {
		let (mut socket, _) = listener.accept().await.unwrap();
		let mut request = [0; 1024];
		let _ = socket.read(&mut request).await;
		let response = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
		socket.write_all(response.as_bytes()).await.unwrap();
	});
	let error = fetch(&url, FETCH_TIMEOUT).await.expect_err("404 accepted");
	assert!(matches!(&error, FetchError::Http(status) if status.contains("404")));

	let closed_port_error = fetch("http://127.0.0.1:9/stack.json", FETCH_TIMEOUT)
		.await
		.expect_err("closed port accepted");
	assert!(matches!(closed_port_error, FetchError::Transport(_)));
}
