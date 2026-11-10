use super::*;
use crate::{
	platform::{HostInfo, PlatformInfo, SigningHostConfig},
	test_support::{StubPlatform, test_spawner},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[test]
fn only_a_dotted_assignment_for_the_requested_base_can_be_confirmed() {
	for username in
		["another.01", "chosen", "chosen.1", "chosen.001", "chosen.aa", "chosen.01.paseo"]
	{
		assert!(assigned_username(&json!({ "username": username }), "chosen").is_err());
	}
	assert!(assigned_username(&json!({ "status": "pending" }), "chosen").is_err());
	assert_eq!(
		assigned_username(&json!({ "username": "chosen.07" }), "chosen").unwrap(),
		"chosen.07"
	);
}

#[tokio::test]
async fn a_backend_response_cannot_cross_a_local_session_replacement() {
	let config = SigningHostConfig::new(
		HostInfo {
			name: "Registration test".to_string(),
			icon: None,
			version: None,
			platform: crate::latest::HostPlatform::Unknown,
		},
		PlatformInfo::default(),
		[0; 32],
		[0xbb; 32],
		[0xcc; 32],
		"testnet".to_string(),
	)
	.unwrap();
	let runtime = Arc::new(SigningHostRuntime::new(
		Arc::new(StubPlatform::default()),
		config,
		test_spawner(),
	));
	runtime.activate_local_session(vec![0xab; 32]).await.unwrap();
	let context = runtime.local_identity_context().unwrap();
	let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
	let address = listener.local_addr().unwrap();
	let (arrived, started) = tokio::sync::oneshot::channel();
	let (release, released) = tokio::sync::oneshot::channel();
	let server = tokio::spawn(async move {
		let (mut socket, _) = listener.accept().await.unwrap();
		let mut request = [0u8; 1024];
		socket.read(&mut request).await.unwrap();
		arrived.send(()).unwrap();
		released.await.unwrap();
		socket
			.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
			.await
			.unwrap();
	});
	let client_runtime = runtime.clone();
	let request = tokio::spawn(async move {
		request_json(
			&client_runtime,
			&context,
			Client::new().get(format!("http://{address}/attester")),
			"attester lookup",
		)
		.await
	});
	started.await.unwrap();
	runtime.activate_local_session(vec![0xcd; 32]).await.unwrap();
	release.send(()).unwrap();
	assert!(matches!(request.await.unwrap(), Err(HostRejection::Rejected { .. })));
	server.await.unwrap();
}
