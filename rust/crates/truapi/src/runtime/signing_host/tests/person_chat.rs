//! The chat product's keys on a signing host.

use super::*;
use crate::host_logic::{
	sso::pairing::{derive_identity_chat_private_key, x25519_public_key},
	statement_store::{statement_fields_from_v01, unsigned_statement_signing_payload},
};
use parity_scale_codec::Encode;
use std::sync::Mutex;
use truapi::{
	api::PersonChat,
	versioned::person_chat::{
		HostPersonChatError, HostPersonChatGetProfileRequest, HostPersonChatGetProfileResponse,
		HostPersonChatHistoryExportRequest, HostPersonChatHistoryExportResponse,
		HostPersonChatHistoryImportedRequest, HostPersonChatNotifyPeerRequest,
		HostPersonChatNotifyPeerResponse, HostPersonChatProveChatRequestRequest,
		HostPersonChatProveChatRequestResponse, HostPersonChatProveStatementRequest,
		HostPersonChatProveStatementResponse, HostPersonChatPublishContactsRequest,
		HostPersonChatPushTokensRequest, HostPersonChatPushTokensResponse,
		HostPersonChatSecretsRequest, HostPersonChatSecretsResponse,
		HostPersonChatSetPushRoutesRequest,
	},
};

const CHAT_PRODUCT: &str = "chat.dot";

fn signed_in(product_id: &str) -> ProductRuntimeHost {
	let (services, authority) = signing_runtime();
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	product_runtime_for(services, authority, product_id)
}

fn profile(
	host: &ProductRuntimeHost,
) -> Result<HostPersonChatGetProfileResponse, CallError<HostPersonChatError>> {
	futures::executor::block_on(host.get_profile(
		&CallContext::default(),
		HostPersonChatGetProfileRequest::V1(v01::HostPersonChatGetProfileRequest {}),
	))
}

fn verifies(public: [u8; 32], message: &[u8], signature: [u8; 64]) -> bool {
	let public = schnorrkel::PublicKey::from_bytes(&public).expect("valid public key");
	let signature = schnorrkel::Signature::from_bytes(&signature).expect("valid signature");
	public.verify_simple(SR25519_SIGNING_CONTEXT, message, &signature).is_ok()
}

#[test]
fn the_chat_product_gets_the_identity_chat_profile() {
	let host = signed_in(CHAT_PRODUCT);
	let identity = derive_identity_keypair(&ENTROPY, TEST_NETWORK_SUFFIX)
		.unwrap()
		.public
		.to_bytes();

	let HostPersonChatGetProfileResponse::V1(profile) = profile(&host).expect("profile");
	let HostPersonChatSecretsResponse::V1(secrets) =
		futures::executor::block_on(host.chat_secrets(
			&CallContext::default(),
			HostPersonChatSecretsRequest::V1(v01::HostPersonChatSecretsRequest {}),
		))
		.expect("secrets");

	// Peers find this identity by its account and reach it on the chat key the
	// native apps derive from the same entropy.
	assert_eq!(
		profile,
		v01::HostPersonChatGetProfileResponse {
			identity_account_id: identity,
			chat_public_key: x25519_public_key(derive_identity_chat_private_key(&ENTROPY)),
			lite_username: None,
			full_username: None,
			role: v01::PersonChatRole::Primary,
			statement_account_id: identity,
			device_enc_public_key: x25519_public_key(secrets.device_enc_secret),
			own_devices: vec![v01::PersonChatDevice {
				statement_account_id: identity,
				enc_public_key: x25519_public_key(secrets.device_enc_secret),
				name: "Polkadot Mobile".to_string(),
			}],
		}
	);
	assert_eq!(secrets.chat_identity_secret, derive_identity_chat_private_key(&ENTROPY));
}

#[test]
fn statements_and_chat_requests_are_signed_by_the_identity_account() {
	let host = signed_in(CHAT_PRODUCT);
	let identity = derive_identity_keypair(&ENTROPY, TEST_NETWORK_SUFFIX)
		.unwrap()
		.public
		.to_bytes();
	let statement = v01::Statement {
		proof: None,
		decryption_key: None,
		expiry: Some(u64::MAX),
		channel: Some([3; 32]),
		topics: vec![[1; 32], [2; 32]],
		data: Some(vec![9; 40]),
	};

	let HostPersonChatProveStatementResponse::V1(proved) =
		futures::executor::block_on(host.prove_statement(
			&CallContext::default(),
			HostPersonChatProveStatementRequest::V1(v01::HostPersonChatProveStatementRequest {
				statement: statement.clone(),
			}),
		))
		.expect("statement proof");
	let v01::StatementProof::Sr25519 { signature, signer } = proved.proof else {
		panic!("expected an sr25519 proof, got {:?}", proved.proof);
	};
	let payload =
		unsigned_statement_signing_payload(statement_fields_from_v01(statement).unwrap()).unwrap();
	// The statement store checks exactly this signature: the native peers
	// sign their statements the same way.
	assert_eq!(signer, identity);
	assert!(verifies(identity, &payload, signature));

	let mut message = "request-1".to_string().encode();
	1_763_164_800_000u64.encode_to(&mut message);
	message.push(1);
	message.extend_from_slice(&identity);
	let acceptor = [5u8; 32];
	let HostPersonChatProveChatRequestResponse::V1(response) =
		futures::executor::block_on(host.prove_chat_request(
			&CallContext::default(),
			HostPersonChatProveChatRequestRequest::V1(v01::HostPersonChatProveChatRequestRequest {
				message: message.clone(),
				acceptor,
			}),
		))
		.expect("chat request proof");
	let mut signed = message;
	signed.push(0x80);
	signed.extend_from_slice(&acceptor);
	assert!(verifies(identity, &signed, response.signature));
}

#[test]
fn only_the_chat_product_reaches_the_identity_keys() {
	for product_id in ["myapp.dot", "app.chat.dot", "chatx.paseo"] {
		let host = signed_in(product_id);
		assert!(
			matches!(profile(&host), Err(CallError::Unsupported)),
			"{product_id} must not reach the chat keys"
		);
	}
	for product_id in ["chat.dot", "chat.paseo", "chat.testnet"] {
		assert!(profile(&signed_in(product_id)).is_ok(), "{product_id} is the chat product");
	}
}

#[test]
fn no_session_is_not_connected() {
	let (services, authority) = signing_runtime();
	let host = product_runtime_for(services, authority, CHAT_PRODUCT);
	assert!(matches!(
		profile(&host),
		Err(CallError::Domain(HostPersonChatError::V1(v01::HostPersonChatError::NotConnected)))
	));
}

/// A push service that has an APNs and a VoIP token and records each delivery.
#[derive(Default)]
struct RecordingPush {
	delivered: Mutex<Vec<crate::platform::HostPushDelivery>>,
	routes: Mutex<Vec<crate::platform::HostChatPushRoute>>,
}

#[truapi::async_trait]
impl crate::platform::PushPlatform for RecordingPush {
	async fn send_push(
		&self,
		_product: &ProductContext,
		delivery: crate::platform::HostPushDelivery,
	) -> Result<bool, truapi::latest::GenericError> {
		self.delivered.lock().unwrap().push(delivery);
		Ok(true)
	}

	async fn push_tokens(
		&self,
	) -> Result<crate::platform::HostPushTokens, truapi::latest::GenericError> {
		Ok(crate::platform::HostPushTokens {
			service: crate::platform::HostPushService::Apns,
			token: Some("a1b2".to_string()),
			voip_token: Some("c3d4".to_string()),
		})
	}

	async fn set_chat_push_routes(
		&self,
		_product: &ProductContext,
		routes: Vec<crate::platform::HostChatPushRoute>,
	) -> Result<(), truapi::latest::GenericError> {
		*self.routes.lock().unwrap() = routes;
		Ok(())
	}
}

fn chat_host_with_push(push: Arc<RecordingPush>) -> ProductRuntimeHost {
	let (services, authority) = signing_runtime();
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	services.install_push_platform(push);
	product_runtime_for(services, authority, CHAT_PRODUCT)
}

#[test]
fn push_tokens_are_the_bytes_a_chat_token_message_carries() {
	let host = chat_host_with_push(Arc::new(RecordingPush::default()));

	let HostPersonChatPushTokensResponse::V1(tokens) =
		futures::executor::block_on(host.push_tokens(
			&CallContext::default(),
			HostPersonChatPushTokensRequest::V1(v01::HostPersonChatPushTokensRequest {}),
		))
		.expect("push tokens");

	// Native peers store the raw APNs bytes, not their hex.
	assert_eq!(
		tokens,
		v01::HostPersonChatPushTokensResponse {
			push: Some(v01::PersonChatPushToken {
				token: vec![0xa1, 0xb2],
				platform: v01::PersonChatPushPlatform::Ios,
			}),
			voip: Some(vec![0xc3, 0xd4]),
		}
	);
}

#[test]
fn a_chat_push_reaches_the_relay_in_the_native_shape() {
	let push = Arc::new(RecordingPush::default());
	let host = chat_host_with_push(push.clone());

	let HostPersonChatNotifyPeerResponse::V1(response) =
		futures::executor::block_on(host.notify_peer(
			&CallContext::default(),
			HostPersonChatNotifyPeerRequest::V1(v01::HostPersonChatNotifyPeerRequest {
				device_token: "fcm-token".to_string(),
				push_id: [0xab; 32],
				platform: v01::PersonChatPushPlatform::Android,
				message: vec![1, 2, 3],
				voip: false,
			}),
		))
		.expect("notify");

	// The receiving native app looks the chat up by the 64-character push id
	// and decodes the hex message itself; the core must pass both unchanged.
	assert!(response.delivered);
	assert_eq!(
		*push.delivered.lock().unwrap(),
		vec![crate::platform::HostPushDelivery {
			service: crate::platform::HostPushService::Fcm,
			token: "fcm-token".to_string(),
			push_id: "ab".repeat(32),
			message: "010203".to_string(),
			voip: false,
		}]
	);
}

#[test]
fn another_product_cannot_relay_chat_pushes() {
	let push = Arc::new(RecordingPush::default());
	let (services, authority) = signing_runtime();
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	services.install_push_platform(push.clone());
	let host = product_runtime_for(services, authority, "myapp.dot");

	let result = futures::executor::block_on(host.notify_peer(
		&CallContext::default(),
		HostPersonChatNotifyPeerRequest::V1(v01::HostPersonChatNotifyPeerRequest {
			device_token: "fcm-token".to_string(),
			push_id: [0xab; 32],
			platform: v01::PersonChatPushPlatform::Android,
			message: vec![1],
			voip: false,
		}),
	));

	assert!(matches!(result, Err(CallError::Unsupported)));
	assert!(push.delivered.lock().unwrap().is_empty());
}

#[test]
fn push_routes_reach_the_host_as_its_notification_receiver_reads_them() {
	let push = Arc::new(RecordingPush::default());
	let host = chat_host_with_push(push.clone());
	let route = |room_id: &str| v01::PersonChatPushRoute {
		push_id: [0xcd; 32],
		room_id: room_id.to_string(),
		title: "a".repeat(80),
		key: [7; 32],
	};

	futures::executor::block_on(host.set_push_routes(
		&CallContext::default(),
		HostPersonChatSetPushRoutesRequest::V1(v01::HostPersonChatSetPushRoutesRequest {
			routes: vec![route("dm-ab")],
		}),
	))
	.expect("routes");

	// The receiver looks the route up by the push id as the push carries it,
	// 64 hex characters, and shows a title no longer than a banner holds.
	assert_eq!(
		*push.routes.lock().unwrap(),
		vec![crate::platform::HostChatPushRoute {
			push_id: "cd".repeat(32),
			room_id: "dm-ab".to_string(),
			title: "a".repeat(64),
			key: vec![7; 32],
		}]
	);

	let refused = futures::executor::block_on(host.set_push_routes(
		&CallContext::default(),
		HostPersonChatSetPushRoutesRequest::V1(v01::HostPersonChatSetPushRoutesRequest {
			routes: vec![route("  ")],
		}),
	));
	assert!(matches!(
		refused,
		Err(CallError::Domain(HostPersonChatError::V1(
			v01::HostPersonChatError::InvalidRequest { .. }
		)))
	));
}

/// Person chat services that record the contacts the product publishes, and
/// serve a staged native chat history.
#[derive(Default)]
struct RecordingPersonChat {
	contacts: Mutex<Vec<(String, Vec<crate::platform::HostChatContact>)>>,
	history: Vec<u8>,
	imported: Mutex<Vec<String>>,
}

#[truapi::async_trait]
impl crate::platform::PersonChatPlatform for RecordingPersonChat {
	async fn hop_nodes(&self) -> Result<Vec<String>, truapi::latest::GenericError> {
		Ok(Vec::new())
	}

	async fn paired_devices(
		&self,
	) -> Result<Vec<crate::platform::HostPairedDevice>, truapi::latest::GenericError> {
		Ok(Vec::new())
	}

	async fn set_contacts(
		&self,
		product: &ProductContext,
		contacts: Vec<crate::platform::HostChatContact>,
	) -> Result<(), truapi::latest::GenericError> {
		self.contacts.lock().unwrap().push((product.product_id.clone(), contacts));
		Ok(())
	}

	async fn history_export(
		&self,
		_product: &ProductContext,
		offset: u64,
		max_len: u32,
	) -> Result<crate::platform::HostHistoryExportChunk, truapi::latest::GenericError> {
		let start = (offset as usize).min(self.history.len());
		let end = (start + max_len as usize).min(self.history.len());
		Ok(crate::platform::HostHistoryExportChunk {
			total: self.history.len() as u64,
			data: self.history[start..end].to_vec(),
		})
	}

	async fn history_imported(
		&self,
		product: &ProductContext,
	) -> Result<(), truapi::latest::GenericError> {
		self.imported.lock().unwrap().push(product.product_id.clone());
		Ok(())
	}
}

fn chat_host_with(platform: Arc<RecordingPersonChat>, product_id: &str) -> ProductRuntimeHost {
	let (services, authority) = signing_runtime();
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	services.install_person_chat_platform(platform);
	product_runtime_for(services, authority, product_id)
}

fn history_part(
	host: &ProductRuntimeHost,
	offset: u64,
) -> Result<v01::HostPersonChatHistoryExportResponse, CallError<HostPersonChatError>> {
	futures::executor::block_on(host.history_export(
		&CallContext::default(),
		HostPersonChatHistoryExportRequest::V1(v01::HostPersonChatHistoryExportRequest { offset }),
	))
	.map(|HostPersonChatHistoryExportResponse::V1(part)| part)
}

#[test]
fn the_chat_product_reads_the_staged_history_in_parts_and_says_when_it_is_imported() {
	let history: Vec<u8> = (0..(1 << 20) + 5).map(|index| (index % 251) as u8).collect();
	let platform = Arc::new(RecordingPersonChat {
		history: history.clone(),
		..RecordingPersonChat::default()
	});
	let host = chat_host_with(platform.clone(), CHAT_PRODUCT);

	// A part is at most 1 MiB, so a large history takes several calls.
	let first = history_part(&host, 0).expect("first part");
	let second = history_part(&host, first.data.len() as u64).expect("second part");
	assert_eq!(
		(first.total, first.data.len(), second.data.len()),
		(history.len() as u64, 1 << 20, 5)
	);
	assert_eq!([first.data, second.data].concat(), history);

	futures::executor::block_on(host.history_imported(
		&CallContext::default(),
		HostPersonChatHistoryImportedRequest::V1(v01::HostPersonChatHistoryImportedRequest {}),
	))
	.expect("imported");
	assert_eq!(*platform.imported.lock().unwrap(), vec![CHAT_PRODUCT.to_string()]);

	// Another product never reads the user's chat history.
	assert!(matches!(
		history_part(&chat_host_with(platform, "notes.dot"), 0),
		Err(CallError::Unsupported)
	));
}

fn publish_contacts(
	host: &ProductRuntimeHost,
	contacts: Vec<v01::PersonChatContact>,
) -> Result<(), CallError<HostPersonChatError>> {
	futures::executor::block_on(host.publish_contacts(
		&CallContext::default(),
		HostPersonChatPublishContactsRequest::V1(v01::HostPersonChatPublishContactsRequest {
			contacts,
		}),
	))
	.map(|_| ())
}

#[test]
fn the_chat_product_gives_the_app_its_contacts_and_no_other_product_can() {
	let recorded = Arc::new(RecordingPersonChat::default());
	let host_for = |product_id: &str| chat_host_with(recorded.clone(), product_id);
	let contact = |room_id: &str, username: Option<String>, blocked: bool| v01::PersonChatContact {
		account_id: [0xab; 32],
		username,
		room_id: room_id.to_string(),
		blocked,
	};

	publish_contacts(
		&host_for(CHAT_PRODUCT),
		vec![contact("contact:ab", Some("b".repeat(80)), true)],
	)
	.expect("contacts");

	// A name longer than the app shows is cut, and a blocked contact stays
	// listed so the app can show it as blocked.
	assert_eq!(
		*recorded.contacts.lock().unwrap(),
		vec![(
			CHAT_PRODUCT.to_string(),
			vec![crate::platform::HostChatContact {
				account_id: [0xab; 32],
				username: Some("b".repeat(64)),
				room_id: "contact:ab".to_string(),
				blocked: true,
			}]
		)]
	);

	// Another product could otherwise put names on accounts in the wallet.
	assert!(matches!(
		publish_contacts(&host_for("notes.dot"), vec![contact("contact:ab", None, false)]),
		Err(CallError::Unsupported)
	));
	assert!(matches!(
		publish_contacts(&host_for(CHAT_PRODUCT), vec![contact(" ", None, false)]),
		Err(CallError::Domain(HostPersonChatError::V1(
			v01::HostPersonChatError::InvalidRequest { .. }
		)))
	));
	assert_eq!(recorded.contacts.lock().unwrap().len(), 1);

	// A host that keeps no contacts says so.
	assert!(matches!(
		publish_contacts(&signed_in(CHAT_PRODUCT), vec![contact("contact:ab", None, false)]),
		Err(CallError::Unsupported)
	));
}

#[test]
fn a_native_chat_device_key_the_host_imported_is_the_chat_device_key() {
	let (services, authority) = signing_runtime();
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("activation succeeds");
	assert_eq!(
		futures::executor::block_on(services.import_chat_device_secret([0x5a; 32])),
		Ok(true)
	);
	let pairing_key =
		futures::executor::block_on(services.device_encryption_secret()).expect("device key");
	let host = product_runtime_for(services, authority, CHAT_PRODUCT);

	let HostPersonChatSecretsResponse::V1(secrets) =
		futures::executor::block_on(host.chat_secrets(
			&CallContext::default(),
			HostPersonChatSecretsRequest::V1(v01::HostPersonChatSecretsRequest {}),
		))
		.expect("secrets");
	let HostPersonChatGetProfileResponse::V1(profile) = profile(&host).expect("profile");

	// Contacts of the native chat address this device by the native key; the
	// key SSO pairing advertises stays as it was.
	assert_eq!(secrets.device_enc_secret, [0x5a; 32]);
	assert_eq!(profile.device_enc_public_key, x25519_public_key([0x5a; 32]));
	assert_ne!(pairing_key, [0x5a; 32]);
}

#[test]
fn chat_keys_never_reach_a_log_line() {
	// Core tracing prints requests and platform values with `Debug`; the X25519
	// secrets and the push route keys must not appear in it.
	let secrets = v01::HostPersonChatSecretsResponse {
		chat_identity_secret: [0xaa; 32],
		device_enc_secret: [0xbb; 32],
	};
	let route = crate::platform::HostChatPushRoute {
		push_id: "cd".repeat(32),
		room_id: "dm-ab".to_string(),
		title: "alice".to_string(),
		key: vec![0xcc; 32],
	};
	let product_route = v01::PersonChatPushRoute {
		push_id: [0; 32],
		room_id: "dm-ab".into(),
		title: "a".into(),
		key: [0xdd; 32],
	};
	for printed in [format!("{secrets:?}"), format!("{route:?}"), format!("{product_route:?}")] {
		assert!(printed.contains("<redacted>"), "{printed}");
		for secret_byte in ["170", "187", "204", "221", "aa", "bb"] {
			assert!(!printed.contains(&format!("{secret_byte}, {secret_byte}")), "{printed}");
		}
	}
}

#[cfg(feature = "test-host")]
mod hop {
	//! HOP files through a stand-in node; the product's allowance is granted
	//! without the chain, which only test hosts can do.

	use super::*;
	use crate::{
		host_logic::product_account::derive_sr25519_hard_path, platform::ProductExecutionKind,
	};
	use std::collections::HashMap;
	use truapi::versioned::person_chat::{
		HostPersonChatHopAckRequest, HostPersonChatHopDownloadRequest,
		HostPersonChatHopDownloadResponse, HostPersonChatHopUploadRequest,
		HostPersonChatHopUploadResponse,
	};

	/// A HOP node as the pool serves the native apps: it checks the submit
	/// signature and the claim and ack signatures of the recipient key, and drops
	/// an entry on ack.
	/// An entry and the recipient key that may claim it.
	#[derive(Clone)]
	struct PoolEntry {
		data: Vec<u8>,
		recipient: [u8; 32],
	}

	#[derive(Default)]
	struct StandInHopNode {
		pool: Mutex<HashMap<[u8; 32], PoolEntry>>,
		signers: Mutex<Vec<[u8; 32]>>,
		acked: Mutex<Vec<[u8; 32]>>,
		dialed: Mutex<Vec<String>>,
	}

	fn multi_key(value: &serde_json::Value) -> Vec<u8> {
		let bytes = hex::decode(value.as_str().unwrap().trim_start_matches("0x")).unwrap();
		assert_eq!(bytes[0], 1, "an sr25519 MultiSigner or MultiSignature");
		bytes[1..].to_vec()
	}

	fn hash_param(value: &serde_json::Value) -> [u8; 32] {
		hex::decode(value.as_str().unwrap().trim_start_matches("0x"))
			.unwrap()
			.try_into()
			.unwrap()
	}

	#[async_trait::async_trait]
	impl crate::runtime::hop_client::HopTransport for StandInHopNode {
		async fn call(
			&self,
			method: &str,
			params: Vec<serde_json::Value>,
		) -> Result<serde_json::Value, crate::runtime::hop_client::HopCallError> {
			use crate::{
				host_logic::hop::{ack_payload, claim_payload, entry_identifier, submit_payload},
				runtime::hop_client::HopCallError,
			};
			let not_found = || HopCallError::Rpc { code: 1004, message: "not found".into() };
			match method {
				"hop_submit" => {
					let data =
						hex::decode(params[0].as_str().unwrap().trim_start_matches("0x")).unwrap();
					let recipient: [u8; 32] = multi_key(&params[1][0]).try_into().unwrap();
					let signature: [u8; 64] = multi_key(&params[2]).try_into().unwrap();
					let signer: [u8; 32] = multi_key(&params[3]).try_into().unwrap();
					let timestamp = params[4].as_u64().unwrap();
					assert!(verifies(signer, &submit_payload(&data, timestamp), signature));
					self.signers.lock().unwrap().push(signer);
					self.pool
						.lock()
						.unwrap()
						.insert(entry_identifier(&data), PoolEntry { data, recipient });
					Ok(serde_json::json!({ "poolStatus": {} }))
				},
				"hop_claim" | "hop_ack" => {
					let identifier = hash_param(&params[0]);
					let signature: [u8; 64] = multi_key(&params[1]).try_into().unwrap();
					let mut pool = self.pool.lock().unwrap();
					let PoolEntry { data, recipient } =
						pool.get(&identifier).cloned().ok_or_else(not_found)?;
					let payload = if method == "hop_claim" {
						claim_payload(&identifier)
					} else {
						ack_payload(&identifier)
					};
					assert!(
						verifies(recipient, &payload, signature),
						"{method} signed by the ticket key"
					);
					if method == "hop_ack" {
						pool.remove(&identifier);
						self.acked.lock().unwrap().push(identifier);
						return Ok(serde_json::Value::Null);
					}
					Ok(serde_json::json!(format!("0x{}", hex::encode(data))))
				},
				"bitswap_v1_get" => {
					Err(HopCallError::Rpc { code: -32810, message: "not found".into() })
				},
				other => panic!("unexpected call {other}"),
			}
		}
	}

	struct StandInHopConnector(Arc<StandInHopNode>);

	#[async_trait::async_trait]
	impl crate::runtime::hop_client::HopConnector for StandInHopConnector {
		async fn connect(
			&self,
			url: &str,
		) -> Result<
			Arc<dyn crate::runtime::hop_client::HopTransport>,
			crate::runtime::hop_client::HopCallError,
		> {
			self.0.dialed.lock().unwrap().push(url.to_string());
			Ok(self.0.clone())
		}
	}

	struct HopNodes(Vec<String>);

	#[async_trait::async_trait]
	impl crate::platform::PersonChatPlatform for HopNodes {
		async fn hop_nodes(&self) -> Result<Vec<String>, truapi::latest::GenericError> {
			Ok(self.0.clone())
		}

		async fn paired_devices(
			&self,
		) -> Result<Vec<crate::platform::HostPairedDevice>, truapi::latest::GenericError> {
			Ok(vec![crate::platform::HostPairedDevice {
				statement_account_id: [0xd1; 32],
				encryption_public_key: [0xd2; 32],
				name: "Desktop".to_string(),
			}])
		}

		async fn set_contacts(
			&self,
			_product: &crate::platform::ProductContext,
			_contacts: Vec<crate::platform::HostChatContact>,
		) -> Result<(), truapi::latest::GenericError> {
			Ok(())
		}

		async fn history_export(
			&self,
			_product: &crate::platform::ProductContext,
			_offset: u64,
			_max_len: u32,
		) -> Result<crate::platform::HostHistoryExportChunk, truapi::latest::GenericError> {
			Ok(crate::platform::HostHistoryExportChunk { total: 0, data: Vec::new() })
		}

		async fn history_imported(
			&self,
			_product: &crate::platform::ProductContext,
		) -> Result<(), truapi::latest::GenericError> {
			Ok(())
		}
	}

	const HOP_NODE: &str = "wss://hop.example/node";

	fn chat_host_with_hop(product_id: &str, node: Arc<StandInHopNode>) -> ProductRuntimeHost {
		let (services, authority) = signing_runtime();
		futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
			.expect("activation succeeds");
		authority.set_grant_allowances_unchecked(true);
		services.install_person_chat_platform(Arc::new(HopNodes(vec![
			"ws://plain.example".to_string(),
			HOP_NODE.to_string(),
		])));
		services.install_hop_connector(Arc::new(StandInHopConnector(node)));
		product_runtime_for(services, authority, product_id)
	}

	fn hop_upload(
		host: &ProductRuntimeHost,
		data: Vec<u8>,
	) -> Result<v01::HostPersonChatHopUploadResponse, CallError<HostPersonChatError>> {
		futures::executor::block_on(host.hop_upload(
			&CallContext::default(),
			HostPersonChatHopUploadRequest::V1(v01::HostPersonChatHopUploadRequest { data }),
		))
		.map(|HostPersonChatHopUploadResponse::V1(response)| response)
	}

	fn hop_download(
		host: &ProductRuntimeHost,
		uploaded: &v01::HostPersonChatHopUploadResponse,
		node: &str,
	) -> Result<v01::HostPersonChatHopDownloadResponse, CallError<HostPersonChatError>> {
		futures::executor::block_on(host.hop_download(
			&CallContext::default(),
			HostPersonChatHopDownloadRequest::V1(v01::HostPersonChatHopDownloadRequest {
				identifier: uploaded.identifier,
				ticket: uploaded.ticket,
				node: node.to_string(),
			}),
		))
		.map(|HostPersonChatHopDownloadResponse::V1(response)| response)
	}

	#[test]
	fn a_file_goes_through_a_hop_node_and_is_dropped_once_acked() {
		let node = Arc::new(StandInHopNode::default());
		let sender = chat_host_with_hop(CHAT_PRODUCT, node.clone());
		let receiver = chat_host_with_hop(CHAT_PRODUCT, node.clone());
		// Over the inline limit, so the file travels as chunks under a root entry.
		let file: Vec<u8> = (0..crate::host_logic::hop::HOP_CHUNK_BYTES + 10)
			.map(|index| index as u8)
			.collect();

		let uploaded = hop_upload(&sender, file.clone()).expect("upload");
		// Only a wss:// node of the host's list is used.
		assert_eq!(uploaded.node, HOP_NODE);
		// The product's Bulletin allowance account signs, which pays if nobody claims.
		let allowance =
			derive_sr25519_hard_path(&ENTROPY, &["allowance", "bulletin", CHAT_PRODUCT])
				.unwrap()
				.public
				.to_bytes();
		// Two chunks, then the root.
		assert_eq!(*node.signers.lock().unwrap(), vec![allowance; 3]);

		let downloaded = hop_download(&receiver, &uploaded, HOP_NODE).expect("download");
		assert_eq!(downloaded.data, file);
		assert_eq!(downloaded.entries.len(), 3);
		assert_eq!(downloaded.entries[0], uploaded.identifier);

		futures::executor::block_on(receiver.hop_ack(
			&CallContext::default(),
			HostPersonChatHopAckRequest::V1(v01::HostPersonChatHopAckRequest {
				node: HOP_NODE.to_string(),
				ticket: uploaded.ticket,
				entries: downloaded.entries.clone(),
			}),
		))
		.expect("ack");
		assert_eq!(*node.acked.lock().unwrap(), downloaded.entries);
		assert!(node.pool.lock().unwrap().is_empty());
		// Neither the pool nor Bulletin has it any more.
		assert!(hop_download(&receiver, &uploaded, HOP_NODE).is_err());
	}

	#[test]
	fn a_node_the_host_does_not_trust_is_never_dialed() {
		let node = Arc::new(StandInHopNode::default());
		let host = chat_host_with_hop(CHAT_PRODUCT, node.clone());
		let uploaded = hop_upload(&host, vec![1; 100]).expect("upload");
		node.dialed.lock().unwrap().clear();

		// A message names the node; one the app does not list could be anyone's server.
		assert!(matches!(
			hop_download(&host, &uploaded, "wss://attacker.example"),
			Err(CallError::Domain(HostPersonChatError::V1(
				v01::HostPersonChatError::InvalidRequest { .. }
			)))
		));
		assert!(node.dialed.lock().unwrap().is_empty());
		assert_eq!(
			hop_upload(&chat_host_with_hop("coinflip.dot", node), vec![1]),
			Err(CallError::Unsupported)
		);
	}

	// The host renders a HOP file through the core, then acks it once cached,
	// as the native chat acks only a stored file.
	#[test]
	fn a_room_shows_a_hop_file_and_acks_it_once_kept() {
		let node = Arc::new(StandInHopNode::default());
		let sender = chat_host_with_hop(CHAT_PRODUCT, node.clone());
		let host = chat_host_with_hop(CHAT_PRODUCT, node.clone());
		let uploaded = hop_upload(&sender, vec![5; 2_000]).expect("upload");
		let resource = truapi::latest::ChatResource::Hop {
			identifier: uploaded.identifier.to_vec(),
			ticket: uploaded.ticket.to_vec(),
			node: uploaded.node.clone(),
			size: 2_000,
		};

		let (bytes, entries) =
			futures::executor::block_on(host.open_hop_chat_resource(&resource)).expect("open");
		assert_eq!(bytes, vec![5; 2_000]);
		assert!(node.acked.lock().unwrap().is_empty(), "nothing is acked before the host keeps it");
		// The sender shows its own photo too, and must not drop it before the contact claims it.
		futures::executor::block_on(sender.ack_hop_chat_resource(&resource, &entries))
			.expect("ack");
		assert!(node.acked.lock().unwrap().is_empty(), "the uploader never acks its own file");
		futures::executor::block_on(host.ack_hop_chat_resource(&resource, &entries)).expect("ack");
		assert_eq!(*node.acked.lock().unwrap(), vec![uploaded.identifier]);

		let elsewhere = truapi::latest::ChatResource::Hop {
			identifier: uploaded.identifier.to_vec(),
			ticket: uploaded.ticket.to_vec(),
			node: "wss://attacker.example".to_string(),
			size: 2_000,
		};
		assert!(futures::executor::block_on(host.open_hop_chat_resource(&elsewhere)).is_err());
		let other_product = chat_host_with_hop("coinflip.dot", node);
		assert!(
			futures::executor::block_on(other_product.open_hop_chat_resource(&resource)).is_err()
		);
	}

	/// A Worker connection of `product_id` whose room `main` has `features`,
	/// on a host that lists the stand-in node.
	fn room_with_features(
		product_id: &str,
		features: truapi::latest::ChatRoomFeatures,
		node: Arc<StandInHopNode>,
	) -> ProductRuntimeHost {
		let (services, authority) = signing_runtime();
		futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
			.expect("activation succeeds");
		authority.set_grant_allowances_unchecked(true);
		services.install_person_chat_platform(Arc::new(HopNodes(vec![HOP_NODE.to_string()])));
		services.install_hop_connector(Arc::new(StandInHopConnector(node)));
		let chat = super::super::chat_staging::FeatureHost::default();
		chat.features.lock().unwrap().insert("main".to_string(), features);
		let mut adapters = crate::host_core::ConnectionAdapters::from_services(&services);
		adapters.chat_platform = Some(Arc::new(chat) as Arc<dyn crate::platform::ChatPlatform>);
		ProductRuntimeHost::from_services(
			services,
			adapters,
			authority.clone(),
			authority,
			ProductContext::new_with_execution(
				product_id.to_string(),
				ProductExecutionKind::Worker,
			)
			.expect("valid product id"),
		)
	}

	// A photo the user sends from the chat product's room goes to a HOP node
	// as the native chat sends it, and the product gets only its reference.
	#[test]
	fn a_user_attachment_of_a_hop_room_goes_to_a_node() {
		let node = Arc::new(StandInHopNode::default());
		let hop_room =
			truapi::latest::ChatRoomFeatures { hop_attachments: true, ..Default::default() };
		let host = room_with_features(CHAT_PRODUCT, hop_room.clone(), node.clone());

		let resource =
			futures::executor::block_on(host.hop_chat_attachment("main", vec![3; 5_000]))
				.expect("upload");
		let truapi::latest::ChatResource::Hop { node: hop_node, size, .. } = &resource else {
			panic!("a HOP room gets a HOP resource, got {resource:?}");
		};
		assert_eq!((hop_node.as_str(), *size), (HOP_NODE, 5_000));
		let (bytes, _) =
			futures::executor::block_on(host.open_hop_chat_resource(&resource)).expect("open");
		assert_eq!(bytes, vec![3; 5_000]);

		let plain_room = room_with_features(CHAT_PRODUCT, Default::default(), node.clone());
		assert_eq!(
			futures::executor::block_on(plain_room.hop_chat_attachment("main", vec![3; 10])),
			Err(crate::runtime::chat_staging::ChatStageRejection::NotDeclared)
		);
		let other_product = room_with_features("coinflip.dot", hop_room, node);
		assert_eq!(
			futures::executor::block_on(other_product.hop_chat_attachment("main", vec![3; 10])),
			Err(crate::runtime::chat_staging::ChatStageRejection::NotDeclared)
		);
	}

	// A chat request goes out under its own delivery account, which a
	// notification slot lets post it, as the native apps send one, so the
	// statement store does not tie the request to the identity.
	#[test]
	fn a_chat_request_is_proved_by_its_delivery_account() {
		let host = chat_host_with_hop(CHAT_PRODUCT, Arc::new(StandInHopNode::default()));
		let truapi::versioned::person_chat::HostPersonChatPrepareDeliveryResponse::V1(prepared) =
			futures::executor::block_on(host.prepare_delivery(
				&CallContext::default(),
				truapi::versioned::person_chat::HostPersonChatPrepareDeliveryRequest::V1(
					v01::HostPersonChatPrepareDeliveryRequest {
						request_id: "6f1d6b2e-0000-4000-8000-000000000001".to_string(),
						size: 600,
					},
				),
			))
			.expect("prepare");
		let v01::PersonChatDelivery::Anonymous { period, account_id } = prepared.delivery else {
			panic!("a test host grants the slot, got {:?}", prepared.delivery);
		};
		let expected = derive_sr25519_hard_path(
			&ENTROPY,
			&["chat-request", "6f1d6b2e-0000-4000-8000-000000000001", &period.to_string()],
		)
		.unwrap()
		.public
		.to_bytes();
		assert_eq!(account_id, expected);

		let statement = v01::Statement {
			proof: None,
			decryption_key: None,
			expiry: Some(u64::MAX),
			channel: None,
			topics: vec![[1; 32]],
			data: Some(vec![9; 40]),
		};
		let truapi::versioned::person_chat::HostPersonChatProveDeliveryResponse::V1(proved) =
			futures::executor::block_on(host.prove_delivery(
				&CallContext::default(),
				truapi::versioned::person_chat::HostPersonChatProveDeliveryRequest::V1(
					v01::HostPersonChatProveDeliveryRequest {
						request_id: "6f1d6b2e-0000-4000-8000-000000000001".to_string(),
						period,
						statement: statement.clone(),
					},
				),
			))
			.expect("prove");
		let v01::StatementProof::Sr25519 { signature, signer } = proved.proof else {
			panic!("an sr25519 proof, got {:?}", proved.proof);
		};
		let payload = crate::host_logic::statement_store::unsigned_statement_signing_payload(
			crate::host_logic::statement_store::statement_fields_from_v01(statement).unwrap(),
		)
		.unwrap();
		assert_eq!(signer, account_id);
		assert!(verifies(account_id, &payload, signature));

		let bad_id = futures::executor::block_on(host.prepare_delivery(
			&CallContext::default(),
			truapi::versioned::person_chat::HostPersonChatPrepareDeliveryRequest::V1(
				v01::HostPersonChatPrepareDeliveryRequest {
					request_id: "../root".to_string(),
					size: 1,
				},
			),
		));
		assert!(matches!(
			bad_id,
			Err(CallError::Domain(HostPersonChatError::V1(
				v01::HostPersonChatError::InvalidRequest { .. }
			)))
		));
		let other = chat_host_with_hop("coinflip.dot", Arc::new(StandInHopNode::default()));
		assert_eq!(
			futures::executor::block_on(other.prepare_delivery(
				&CallContext::default(),
				truapi::versioned::person_chat::HostPersonChatPrepareDeliveryRequest::V1(
					v01::HostPersonChatPrepareDeliveryRequest {
						request_id: "r1".to_string(),
						size: 1
					},
				),
			)),
			Err(CallError::Unsupported)
		);
	}

	// The wallet device announces its paired devices to contacts as the native
	// apps do, so contacts reach the desktop with the key of its SSO session.
	#[test]
	fn the_wallet_profile_lists_its_paired_devices() {
		let host = chat_host_with_hop(CHAT_PRODUCT, Arc::new(StandInHopNode::default()));
		let HostPersonChatGetProfileResponse::V1(profile) = profile(&host).expect("profile");
		assert_eq!(profile.own_devices.len(), 2);
		assert_eq!(
			profile.own_devices[1],
			v01::PersonChatDevice {
				statement_account_id: [0xd1; 32],
				enc_public_key: [0xd2; 32],
				name: "Desktop".to_string(),
			}
		);
	}
}
