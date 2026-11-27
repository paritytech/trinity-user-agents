use super::*;
use tokio::{
	io::{AsyncReadExt as _, AsyncWriteExt as _},
	net::TcpListener,
};

use crate::chain::ChainBackend;

/// The availability response is a flat `{base: status}` map.
#[test]
fn availability_reads_flat_and_versioned_status_maps() {
	let flat = json!({ "pntest": "EXHAUSTED", "other": "AVAILABLE" });
	assert_eq!(availability_status(&flat, "pntest"), Some("EXHAUSTED"));
	assert_eq!(availability_status(&flat, "other"), Some("AVAILABLE"));
	assert_eq!(availability_status(&flat, "missing"), None);

	let versioned = json!({
		"_tag": "v1",
		"value": {
			"pntest": { "status": "AVAILABLE", "availableDigits": [1, 2, 3] }
		}
	});
	assert_eq!(availability_status(&versioned, "pntest"), Some("AVAILABLE"));
	assert_eq!(availability_status(&versioned, "missing"), None);
}

#[test]
fn search_username_restores_the_v1_two_digit_discriminator() {
	assert_eq!(normalize_searched_username("alice.1"), "alice.01");
	assert_eq!(normalize_searched_username("alice.42"), "alice.42");
	assert_eq!(normalize_searched_username("fullperson"), "fullperson");
	assert_eq!(normalize_searched_username("future.123"), "future.123");
}

#[test]
fn backend_search_uses_only_assigned_rows_for_the_candidate() -> Result<()> {
	let page: UsernameSearchPage = serde_json::from_value(json!({
		"usernames": [
			{ "accountId": "candidate", "username": "alice.1", "status": "ASSIGNED" },
			{ "accountId": "candidate", "username": "stale.2", "status": "FAILED" },
			{ "accountId": "someone-else", "username": "other.3", "status": "ASSIGNED" }
		],
		"nextCursor": null
	}))?;

	assert_eq!(
		matching_assigned_usernames(page.usernames, "candidate").collect::<Vec<_>>(),
		["alice.01"]
	);
	Ok(())
}

/// A failed registration leaves the name free, so the listing shows each row's
/// status, and a one-letter prefix must not flood the transcript.
#[test]
fn search_listing_shows_status_and_caps_long_results() -> Result<()> {
	let page: UsernameSearchPage = serde_json::from_value(json!({
		"usernames": [
			{ "accountId": "5Alice", "username": "alice.1", "status": "ASSIGNED" },
			{ "accountId": "5Alicia", "username": "alicia.12", "status": "FAILED" }
		],
		"nextCursor": null
	}))?;
	assert_eq!(
		format_username_matches("ali", &page.usernames),
		"alice.01   ASSIGNED  5Alice\nalicia.12  FAILED    5Alicia"
	);
	assert_eq!(format_username_matches("zz", &[]), "No username starts with \"zz\".");

	let many: Vec<UsernameSearchItem> = serde_json::from_value(json!(
		(0..SEARCH_DISPLAY_LIMIT + 2)
			.map(
				|index| json!({ "accountId": "5", "username": format!("a{index}.01"), "status": "ASSIGNED" })
			)
			.collect::<Vec<_>>()
	))?;
	let listing = format_username_matches("a", &many);
	assert_eq!(listing.lines().count(), SEARCH_DISPLAY_LIMIT + 1);
	assert_eq!(listing.lines().last(), Some("… and 2 more; narrow the prefix"));
	Ok(())
}

#[tokio::test]
async fn username_registration_authenticates_as_the_candidate() -> Result<()> {
	let listener = TcpListener::bind("127.0.0.1:0").await?;
	let dub = format!("http://{}/", listener.local_addr()?);
	let server = tokio::spawn(serve_candidate_bound_registration(listener));

	let entropy = RootEntropy::new([7u8; 16]);
	let registration = build_lite_registration(
		&entropy,
		"paseo",
		[9u8; 32],
		"testing",
		None,
		123,
		SigningContexts { people: None, asset_hub: None },
	)?;
	let client = reqwest::Client::builder().timeout(Duration::from_secs(30)).build()?;
	let network = crate::network::Network::default()
		.bundled_stack(ChainBackend::Rpc)
		.with_dub(&dub);
	let config = AttestConfig {
		network: &network,
		entropy,
		username_base: "testing".to_string(),
		reserved_username: None,
	};
	let result = submit_registration(&client, &config, 123, &registration).await;
	let requests = server.await??;

	assert_eq!(requests.len(), 3);
	assert!(
		result.is_ok(),
		"candidate-bound backend rejected registration: {}",
		result.unwrap_err()
	);
	Ok(())
}

async fn serve_candidate_bound_registration(listener: TcpListener) -> Result<Vec<String>> {
	let mut requests = Vec::new();
	let mut authenticated_account = None;
	for _ in 0..3 {
		let (mut stream, _) = listener.accept().await?;
		let mut buffer = vec![0u8; 32 * 1024];
		let read = stream.read(&mut buffer).await?;
		let request = String::from_utf8_lossy(&buffer[..read]).to_string();
		let lowercase = request.to_ascii_lowercase();
		let (status, body) = if request.starts_with("POST /api/v1/auth/challenges ") {
			("200 OK", r#"{"challenge":"Y2hhbGxlbmdl"}"#.to_string())
		} else if request.starts_with("POST /api/v1/auth/token ") {
			let client_id = request_header(&request, "Auth-ClientId")
				.context("auth token request missing Auth-ClientId")?;
			let bytes = BASE64.decode(client_id).context("Auth-ClientId is not base64")?;
			let public_key: [u8; 32] = bytes.try_into().map_err(|bytes: Vec<u8>| {
				anyhow::anyhow!("Auth-ClientId is {} bytes", bytes.len())
			})?;
			authenticated_account = Some(product_public_key_to_address(public_key));
			("200 OK", r#"{"token":"candidate-token"}"#.to_string())
		} else if request.starts_with("POST /api/v1/usernames ") &&
			lowercase.contains("authorization: bearer candidate-token")
		{
			let payload = request
				.split_once("\r\n\r\n")
				.map(|(_, body)| body)
				.context("username request missing body")?;
			let candidate = serde_json::from_str::<Value>(payload)?
				.get("candidateAccountId")
				.and_then(Value::as_str)
				.context("username request missing candidateAccountId")?
				.to_string();
			if authenticated_account.as_deref() == Some(candidate.as_str()) {
				("202 Accepted", r#"{"status":"PENDING"}"#.to_string())
			} else {
				(
					"403 Forbidden",
					r#"{"error":"candidateAccountId must be the authenticated account."}"#
						.to_string(),
				)
			}
		} else {
			("500 Internal Server Error", r#"{"error":"unexpected request"}"#.to_string())
		};
		let response = format!(
			"HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
			body.len()
		);
		stream.write_all(response.as_bytes()).await?;
		requests.push(request);
	}
	Ok(requests)
}

fn request_header<'a>(request: &'a str, expected: &str) -> Option<&'a str> {
	request.lines().find_map(|line| {
		let (name, value) = line.split_once(':')?;
		name.eq_ignore_ascii_case(expected).then(|| value.trim())
	})
}

#[test]
fn rejected_token_eviction_is_scoped_to_the_matching_cached_value() {
	const BASE: &str = "test://rejected-token";
	const OTHER_BASE: &str = "test://other-token";
	const SUBJECT: [u8; 32] = [1; 32];
	const OTHER_SUBJECT: [u8; 32] = [2; 32];
	let mut tokens = BACKEND_TOKENS.lock().expect("backend token cache mutex poisoned");
	tokens.retain(|token| token.backend_base != BASE && token.backend_base != OTHER_BASE);
	tokens.push(CachedBackendToken {
		backend_base: BASE.to_string(),
		auth_client_id: SUBJECT,
		value: "stale".to_string(),
	});
	tokens.push(CachedBackendToken {
		backend_base: BASE.to_string(),
		auth_client_id: OTHER_SUBJECT,
		value: "other-subject".to_string(),
	});
	tokens.push(CachedBackendToken {
		backend_base: OTHER_BASE.to_string(),
		auth_client_id: SUBJECT,
		value: "keep".to_string(),
	});
	drop(tokens);

	evict_rejected_backend_token(BASE, &SUBJECT, "newer-token");
	assert!(BACKEND_TOKENS.lock().expect("backend token cache mutex poisoned").iter().any(
		|token| {
			token.backend_base == BASE && token.auth_client_id == SUBJECT && token.value == "stale"
		}
	));

	evict_rejected_backend_token(BASE, &SUBJECT, "stale");
	let mut tokens = BACKEND_TOKENS.lock().expect("backend token cache mutex poisoned");
	assert!(
		tokens
			.iter()
			.all(|token| token.backend_base != BASE || token.auth_client_id != SUBJECT)
	);
	assert!(tokens.iter().any(|token| {
		token.backend_base == BASE &&
			token.auth_client_id == OTHER_SUBJECT &&
			token.value == "other-subject"
	}));
	assert!(tokens.iter().any(|token| {
		token.backend_base == OTHER_BASE && token.auth_client_id == SUBJECT && token.value == "keep"
	}));
	tokens.retain(|token| token.backend_base != BASE && token.backend_base != OTHER_BASE);
}

#[tokio::test]
async fn cached_401_reauthenticates_and_retries_once() -> Result<()> {
	let listener = TcpListener::bind("127.0.0.1:0").await?;
	let backend_base = format!("http://{}/api/v1", listener.local_addr()?);
	let server = tokio::spawn(serve_auth_retry(listener));
	let entropy = RootEntropy::new([11u8; 16]);
	let auth_client_id = entropy.derive_identity_keypair("paseo")?.public.to_bytes();

	{
		let mut tokens = BACKEND_TOKENS.lock().expect("backend token cache mutex poisoned");
		tokens.retain(|token| token.backend_base != backend_base);
		tokens.push(CachedBackendToken {
			backend_base: backend_base.clone(),
			auth_client_id,
			value: "expired-test-token".to_string(),
		});
	}

	let client = reqwest::Client::builder().timeout(Duration::from_secs(30)).build()?;
	let url = format!("{backend_base}/protected");
	let response = send_with_backend_auth(&client, &backend_base, &entropy, "paseo", |token| {
		client.post(&url).bearer_auth(token).body("{}")
	})
	.await?;
	assert_eq!(response.status(), reqwest::StatusCode::OK);
	let requests = server.await??;
	assert_eq!(requests.len(), 4);
	assert!(requests[0].contains("authorization: Bearer expired-test-token"));
	assert!(requests[1].starts_with("POST /api/v1/auth/challenges "));
	assert!(requests[2].starts_with("POST /api/v1/auth/token "));
	let encoded_client_id = BASE64.encode(auth_client_id);
	assert_eq!(request_header(&requests[2], "Auth-ClientId"), Some(encoded_client_id.as_str()));
	assert!(requests[3].contains("authorization: Bearer fresh-token"));

	let mut tokens = BACKEND_TOKENS.lock().expect("backend token cache mutex poisoned");
	assert!(tokens.iter().any(|token| {
		token.backend_base == backend_base &&
			token.auth_client_id == auth_client_id &&
			token.value == "fresh-token"
	}));
	tokens.retain(|token| token.backend_base != backend_base);
	Ok(())
}

async fn serve_auth_retry(listener: TcpListener) -> Result<Vec<String>> {
	let mut requests = Vec::new();
	for _ in 0..4 {
		let (mut stream, _) = listener.accept().await?;
		let mut buffer = vec![0u8; 16 * 1024];
		let read = stream.read(&mut buffer).await?;
		let request = String::from_utf8_lossy(&buffer[..read]).to_string();
		let lowercase = request.to_ascii_lowercase();
		let (status, body) = if request.starts_with("POST /api/v1/protected ") &&
			lowercase.contains("authorization: bearer expired-test-token")
		{
			("401 Unauthorized", r#"{"title":"Invalid Token"}"#)
		} else if request.starts_with("POST /api/v1/auth/challenges ") {
			("200 OK", r#"{"challenge":"Y2hhbGxlbmdl"}"#)
		} else if request.starts_with("POST /api/v1/auth/token ") {
			("200 OK", r#"{"token":"fresh-token"}"#)
		} else if request.starts_with("POST /api/v1/protected ") &&
			lowercase.contains("authorization: bearer fresh-token")
		{
			("200 OK", r#"{"ok":true}"#)
		} else {
			("500 Internal Server Error", r#"{"error":"unexpected request"}"#)
		};
		let response = format!(
			"HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
			body.len()
		);
		stream.write_all(response.as_bytes()).await?;
		requests.push(request);
	}
	Ok(requests)
}
