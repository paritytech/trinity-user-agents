//! Product-facing push capability adapter.
//!
//! A push address is the version byte [`ADDRESS_VERSION`] and the SCALE of
//! [`PushAddress`]. A notification is the JSON object `{v, title, body}`,
//! encrypted with XChaCha20-Poly1305 under the key of the address, with the
//! associated data [`PUSH_AAD`]; the push carries the hex of the nonce and
//! the ciphertext. The receiving device finds the key and the room by the push
//! id.

use chacha20poly1305::{
	KeyInit, XChaCha20Poly1305, XNonce,
	aead::{Aead, Payload},
};
use parity_scale_codec::{Decode, Encode};
use tracing::instrument;
use truapi::{
	CallContext, CallError,
	api::Push,
	latest,
	versioned::{
		FromLatest, IntoLatest, Versioned,
		push::{
			HostPushAddressError, HostPushAddressRequest, HostPushAddressResponse,
			HostPushNotifyError, HostPushNotifyRequest, HostPushNotifyResponse,
		},
	},
};

use super::from_latest_error;
use crate::{
	platform::{HostPushDelivery, HostPushService, normalize_chat_identifier},
	runtime::{ProductRuntimeHost, until_cancelled},
	unix_time::current_unix_secs,
};

/// The version byte of a push address.
pub const ADDRESS_VERSION: u8 = 1;

/// The associated data of an encrypted notification.
pub const PUSH_AAD: &[u8] = b"truapi/push/v1";

/// The most Unicode scalar values of the title of a notification.
pub const PUSH_TITLE_MAX_CHARS: usize = 64;

/// The most Unicode scalar values of the text of a notification.
pub const PUSH_BODY_MAX_CHARS: usize = 256;

/// A product sends at most this many notifications in [`PUSH_WINDOW_SECS`].
pub const PUSH_MAX_PER_WINDOW: usize = 30;

/// The window of the rate limit of `push.notify`.
pub const PUSH_WINDOW_SECS: u64 = 60;

/// The longest push token in an address.
const TOKEN_MAX_BYTES: usize = 512;

/// The device and the room that a push address names.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
struct PushAddress {
	service: HostPushService,
	token: String,
	push_id: String,
	key: [u8; 32],
}

impl PushAddress {
	fn encode_address(&self) -> Vec<u8> {
		let mut bytes = vec![ADDRESS_VERSION];
		self.encode_to(&mut bytes);
		bytes
	}

	fn decode_address(bytes: &[u8]) -> Option<Self> {
		let (&version, mut rest) = bytes.split_first()?;
		if version != ADDRESS_VERSION {
			return None;
		}
		let address = Self::decode(&mut rest).ok()?;
		let valid = rest.is_empty() &&
			!address.token.is_empty() &&
			address.token.len() <= TOKEN_MAX_BYTES &&
			is_push_id(&address.push_id);
		valid.then_some(address)
	}
}

/// Whether `push_id` is 32 lowercase hex characters.
fn is_push_id(push_id: &str) -> bool {
	push_id.len() == 32 && push_id.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The hex of a random nonce and the ciphertext of `{v, title, body}`.
fn encrypt_notification(key: &[u8; 32], title: &str, body: &str) -> Result<String, String> {
	let plaintext = serde_json::json!({ "v": 1, "title": title, "body": body }).to_string();
	let mut nonce = [0u8; 24];
	getrandom::getrandom(&mut nonce).map_err(|error| format!("no random nonce: {error}"))?;
	let ciphertext = XChaCha20Poly1305::new(key.into())
		.encrypt(XNonce::from_slice(&nonce), Payload { msg: plaintext.as_bytes(), aad: PUSH_AAD })
		.map_err(|_| "the notification did not encrypt".to_string())?;
	Ok(hex::encode([nonce.as_slice(), &ciphertext].concat()))
}

#[truapi::async_trait]
impl Push for ProductRuntimeHost {
	#[instrument(skip_all, fields(runtime.method = "push.address"))]
	async fn address(
		&self,
		cx: &CallContext,
		request: HostPushAddressRequest,
	) -> Result<HostPushAddressResponse, CallError<HostPushAddressError>> {
		let version = request.version();
		let request = request.into_latest();
		let result = async {
			let platform = self.services.push_platform().ok_or(CallError::Unsupported)?;
			let room_id =
				normalize_chat_identifier("roomId", &request.room_id).map_err(|error| {
					CallError::Domain(latest::HostPushAddressError::InvalidRequest {
						reason: error.to_string(),
					})
				})?;
			let unknown = |reason: String| {
				CallError::Domain(latest::HostPushAddressError::Unknown { reason })
			};
			let route = until_cancelled(cx, platform.push_route(&self.product, &room_id))
				.await
				.map_err(|error| unknown(error.to_string()))?
				.map_err(|error| unknown(error.reason))?
				.ok_or_else(|| {
					CallError::Domain(latest::HostPushAddressError::NotAvailable {
						reason: "the device has no push token".into(),
					})
				})?;
			let key: [u8; 32] = route
				.key
				.try_into()
				.map_err(|_| unknown("the host gave a key that is not 32 bytes".into()))?;
			if route.token.is_empty() || route.token.len() > TOKEN_MAX_BYTES {
				return Err(unknown("the host gave no valid push token".into()));
			}
			if !is_push_id(&route.push_id) {
				return Err(unknown(
					"the host gave a push id that is not 32 hex characters".into(),
				));
			}
			let address = PushAddress {
				service: route.service,
				token: route.token,
				push_id: route.push_id,
				key,
			};
			Ok(latest::HostPushAddressResponse { address: address.encode_address() })
		}
		.await;
		result
			.map(|response| HostPushAddressResponse::from_latest(response, version))
			.map_err(|error| from_latest_error(error, version))
	}

	/// Checked in this order so a host without a relay, or a request the
	/// product could not make, counts nothing against the rate limit.
	#[instrument(skip_all, fields(runtime.method = "push.notify"))]
	async fn notify(
		&self,
		cx: &CallContext,
		request: HostPushNotifyRequest,
	) -> Result<HostPushNotifyResponse, CallError<HostPushNotifyError>> {
		let version = request.version();
		let request = request.into_latest();
		let result = async {
			let platform = self.services.push_platform().ok_or(CallError::Unsupported)?;
			let address = PushAddress::decode_address(&request.address)
				.ok_or(CallError::Domain(latest::HostPushNotifyError::InvalidAddress))?;
			let invalid = |reason: String| {
				CallError::Domain(latest::HostPushNotifyError::InvalidRequest { reason })
			};
			if request.title.chars().count() > PUSH_TITLE_MAX_CHARS {
				return Err(invalid(format!(
					"the title is longer than {PUSH_TITLE_MAX_CHARS} characters"
				)));
			}
			if request.body.chars().count() > PUSH_BODY_MAX_CHARS {
				return Err(invalid(format!(
					"the text is longer than {PUSH_BODY_MAX_CHARS} characters"
				)));
			}
			let admitted = self.services.admit_push(
				&self.product.product_id,
				current_unix_secs(),
				PUSH_WINDOW_SECS,
				PUSH_MAX_PER_WINDOW,
			);
			if !admitted {
				return Err(CallError::Domain(latest::HostPushNotifyError::RateLimited));
			}
			let unknown =
				|reason: String| CallError::Domain(latest::HostPushNotifyError::Unknown { reason });
			let message = encrypt_notification(&address.key, &request.title, &request.body)
				.map_err(unknown)?;
			let delivery = HostPushDelivery {
				service: address.service,
				token: address.token,
				push_id: address.push_id,
				message,
			};
			let taken = until_cancelled(cx, platform.send_push(&self.product, delivery))
				.await
				.map_err(|error| unknown(error.to_string()))?
				.map_err(|error| unknown(error.reason))?;
			if !taken {
				return Err(CallError::Domain(latest::HostPushNotifyError::Rejected {
					reason: "the push service did not take the notification".into(),
				}));
			}
			Ok(latest::HostPushNotifyResponse {})
		}
		.await;
		result
			.map(|response| HostPushNotifyResponse::from_latest(response, version))
			.map_err(|error| from_latest_error(error, version))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn an_address_reads_back_and_another_version_or_a_bad_id_does_not() {
		let address = PushAddress {
			service: HostPushService::Apns,
			token: "ab".repeat(32),
			push_id: "0123456789abcdef0123456789abcdef".into(),
			key: [7; 32],
		};
		let bytes = address.encode_address();

		assert_eq!(PushAddress::decode_address(&bytes), Some(address.clone()));
		let mut other = bytes.clone();
		other[0] = 2;
		assert_eq!(PushAddress::decode_address(&other), None);
		let bad_id = PushAddress { push_id: "XYZ".into(), ..address };
		assert_eq!(PushAddress::decode_address(&bad_id.encode_address()), None);
		assert_eq!(PushAddress::decode_address(&[]), None);
	}

	#[test]
	fn a_notification_opens_with_the_key_of_the_address_only() {
		let key = [9u8; 32];
		let message =
			hex::decode(encrypt_notification(&key, "alice.42", "hello").unwrap()).unwrap();
		let (nonce, ciphertext) = message.split_at(24);

		let opened = XChaCha20Poly1305::new((&key).into())
			.decrypt(XNonce::from_slice(nonce), Payload { msg: ciphertext, aad: PUSH_AAD })
			.unwrap();
		let json: serde_json::Value = serde_json::from_slice(&opened).unwrap();
		assert_eq!(json, serde_json::json!({ "v": 1, "title": "alice.42", "body": "hello" }));
		assert!(
			XChaCha20Poly1305::new((&[1u8; 32]).into())
				.decrypt(XNonce::from_slice(nonce), Payload { msg: ciphertext, aad: PUSH_AAD })
				.is_err()
		);
	}
}
