//! Staging of chat attachments.
//!
//! A user attachment above the inline bound goes to the Bulletin chain as one
//! preimage, and the message carries its key. The product's own Bulletin
//! allowance pays for it, under the product's `PreimageSubmit` permission, and
//! only in a room whose features declare `preimage_attachments`. The host asks
//! the core to stage an attachment. Neither the host nor the product holds the
//! allowance key.
//!
//! A room that declares `sealed_attachments` gets the attachment encrypted
//! instead: the core seals the bytes with a new key, stores the ciphertext in
//! parts of at most one Bulletin transaction each, and gives the product a
//! `ChatResource::Sealed`. Only the key opens the parts, and the product sends
//! the key through its own encrypted channel. The core opens a sealed resource
//! for the host that draws it.
//!
//! The core reads the declaration of the room from what it saw itself: each
//! `set_room_features` it accepted. When it saw none since the runtime
//! started, it reads the features the host keeps through the chat adapter of
//! the product. With neither, the room declares nothing.

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;
use std::{
	collections::HashMap,
	sync::{Mutex, PoisonError},
};

use tracing::{instrument, warn};
use truapi::{CallContext, latest};
#[cfg(target_arch = "wasm32")]
use web_time::Instant;

use chacha20poly1305::{
	KeyInit, XChaCha20Poly1305, XNonce,
	aead::{Aead, Payload},
};
use futures::StreamExt;
use sha2::{Digest, Sha512};

use super::{PREIMAGE_SUBMIT_TIMEOUT, ProductPreimageError, ProductRuntimeHost};
use crate::{
	host_internal::bulletin::preimage_key,
	platform::{
		CHAT_PREIMAGE_ATTACHMENT_MAX_BYTES, CHAT_SEALED_ATTACHMENT_MAX_BYTES,
		CHAT_SEALED_PARTS_MAX, normalize_chat_identifier,
	},
};

/// The associated data of a sealed attachment. It binds the ciphertext to
/// this use.
pub const CHAT_SEALED_AAD: &[u8] = b"truapi/chat/sealed/v1";

/// The longest wait for one part of a sealed resource.
const SEALED_PART_WAIT: core::time::Duration = core::time::Duration::from_secs(60);

/// Why the core did not open a sealed resource.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display, derive_more::Error)]
pub enum ChatOpenRejection {
	/// The resource is not a well-formed sealed resource.
	#[display("the resource is not a sealed resource: {reason}")]
	Invalid {
		/// Human-readable reason.
		reason: String,
	},
	/// A part did not arrive in time.
	#[display("part {index} of the sealed resource did not arrive")]
	Missing {
		/// The index of the part.
		index: u32,
	},
	/// The parts do not match the digest, or the key does not open them.
	#[display("the sealed resource does not match its digest or key")]
	Mismatch,
}

/// The attachment kinds that a room declares.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct StagingFlags {
	/// `preimage_attachments`.
	preimage: bool,
	/// `sealed_attachments`.
	sealed: bool,
}

impl From<&latest::ChatRoomFeatures> for StagingFlags {
	fn from(features: &latest::ChatRoomFeatures) -> Self {
		Self { preimage: features.preimage_attachments, sealed: features.sealed_attachments }
	}
}

/// Why the core did not stage a chat attachment.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display, derive_more::Error)]
pub enum ChatStageRejection {
	/// The room does not declare `preimage_attachments`, or, for a sealed
	/// attachment, `sealed_attachments`.
	#[display("the room does not declare preimage attachments")]
	NotDeclared,
	/// The attachment is larger than the core stages, or than one Bulletin
	/// transaction stores.
	#[display("the attachment is larger than {max_bytes} bytes")]
	TooLarge {
		/// The largest attachment the core stages now, in bytes.
		max_bytes: u32,
	},
	/// The user refused `PreimageSubmit` for the product.
	#[display("the user refused preimage submission for the product")]
	PermissionDenied,
	/// No Bulletin allowance is available for the product.
	#[display("no Bulletin allowance for the product: {reason}")]
	AllowanceUnavailable {
		/// Human-readable reason.
		reason: String,
	},
	/// The submission failed.
	#[display("the preimage submission failed: {reason}")]
	SubmitFailed {
		/// Human-readable reason.
		reason: String,
	},
}

/// The attachment flags of each room, as the core last saw the room's
/// features, by product and room. Shared by every connection of one host
/// runtime.
#[derive(Default)]
pub struct ChatStagingDeclarations {
	rooms: Mutex<HashMap<(String, String), StagingFlags>>,
}

impl ChatStagingDeclarations {
	/// Keep the flags that `features` of `room_id` declare now.
	pub fn record(&self, product_id: &str, room_id: &str, features: &latest::ChatRoomFeatures) {
		self.lock()
			.insert((product_id.to_string(), room_id.to_string()), features.into());
	}

	/// Forget a room that is gone.
	pub fn forget(&self, product_id: &str, room_id: &str) {
		self.lock().remove(&(product_id.to_string(), room_id.to_string()));
	}

	/// The flags the core last saw, or `None` when it saw none.
	fn declared(&self, product_id: &str, room_id: &str) -> Option<StagingFlags> {
		self.lock().get(&(product_id.to_string(), room_id.to_string())).copied()
	}

	fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<(String, String), StagingFlags>> {
		self.rooms.lock().unwrap_or_else(PoisonError::into_inner)
	}
}

impl ProductRuntimeHost {
	/// Stage `bytes`, a user attachment of `room_id`, as one Bulletin preimage
	/// that the product's own allowance pays for, and answer its key.
	///
	/// The core refuses an attachment above
	/// [`CHAT_PREIMAGE_ATTACHMENT_MAX_BYTES`] or above the per-transaction
	/// bound of the Bulletin runtime, a room that does not declare
	/// `preimage_attachments`, and a product without `PreimageSubmit`. It
	/// prompts for the permission when the user did not decide yet.
	#[instrument(skip_all, fields(runtime.method = "chat.stage_attachment", size = bytes.len()))]
	pub async fn stage_chat_attachment(
		&self,
		room_id: &str,
		mime_type: &str,
		bytes: Vec<u8>,
	) -> Result<Vec<u8>, ChatStageRejection> {
		let too_large = |max_bytes: usize| ChatStageRejection::TooLarge {
			max_bytes: u32::try_from(max_bytes).unwrap_or(u32::MAX),
		};
		if bytes.len() > CHAT_PREIMAGE_ATTACHMENT_MAX_BYTES {
			return Err(too_large(CHAT_PREIMAGE_ATTACHMENT_MAX_BYTES));
		}
		let room_id = normalize_chat_identifier("roomId", room_id)
			.map_err(|_| ChatStageRejection::NotDeclared)?;
		if !self.room_staging(&room_id).await.preimage {
			return Err(ChatStageRejection::NotDeclared);
		}
		self.connection
			.require_remote_permission(latest::RemotePermission::PreimageSubmit, ())
			.await
			.map_err(|error| match error {
				truapi::CallError::HostFailure { reason } => {
					ChatStageRejection::SubmitFailed { reason }
				},
				_ => ChatStageRejection::PermissionDenied,
			})?;
		let Some(session) = self.accounts.current_session() else {
			return Err(ChatStageRejection::AllowanceUnavailable {
				reason: "No active session".to_string(),
			});
		};
		let submission_deadline = Instant::now() + PREIMAGE_SUBMIT_TIMEOUT;
		if !self.submits_preimages_locally() {
			let bound = self
				.connection
				.services
				.bulletin
				.max_transaction_size(submission_deadline)
				.await
				.map_err(|error| ChatStageRejection::SubmitFailed { reason: error.to_string() })?;
			if bytes.len() > bound as usize {
				return Err(too_large(bound as usize));
			}
		}
		tracing::debug!(%room_id, %mime_type, "staging a chat attachment");
		self.store_product_preimage(&CallContext::default(), &session, bytes, submission_deadline)
			.await
			.map_err(|error| match error {
				ProductPreimageError::Allowance(reason) => {
					ChatStageRejection::AllowanceUnavailable { reason }
				},
				ProductPreimageError::Submit(reason) => ChatStageRejection::SubmitFailed { reason },
			})
	}

	/// Keep the attachment flags of features the core accepted for
	/// `room_id`.
	pub fn record_chat_staging(&self, room_id: &str, features: &latest::ChatRoomFeatures) {
		self.connection.services.chat_staging.record(
			&self.connection.product.product_id,
			room_id,
			features,
		);
	}

	/// Forget the staging flag of a deleted room.
	pub fn forget_chat_staging(&self, room_id: &str) {
		self.connection
			.services
			.chat_staging
			.forget(&self.connection.product.product_id, room_id);
	}

	/// The attachment kinds that `room_id` declares: as the core last saw
	/// them, or else as the chat adapter of this connection reads them from
	/// the host.
	async fn room_staging(&self, room_id: &str) -> StagingFlags {
		if let Some(declared) = self
			.connection
			.services
			.chat_staging
			.declared(&self.connection.product.product_id, room_id)
		{
			return declared;
		}
		let Some(platform) = self.connection.chat_platform.as_ref() else {
			return StagingFlags::default();
		};
		match platform
			.read_chat_room_features(&self.connection.product, room_id.to_string())
			.await
		{
			Ok(features) => {
				self.record_chat_staging(room_id, &features);
				(&features).into()
			},
			Err(error) => {
				warn!(reason = %error.reason, "chat room features could not be read");
				StagingFlags::default()
			},
		}
	}

	/// Seal `bytes`, a user attachment of `room_id`: encrypt them with a new
	/// key, store the ciphertext as Bulletin preimages of at most one
	/// transaction each, which the product's own allowance pays for, and
	/// answer the `ChatResource::Sealed` that names them.
	///
	/// The core refuses an attachment above
	/// [`CHAT_SEALED_ATTACHMENT_MAX_BYTES`], a room that does not declare
	/// `sealed_attachments`, and a product without `PreimageSubmit`. It
	/// prompts for the permission when the user did not decide yet.
	#[instrument(skip_all, fields(runtime.method = "chat.seal_attachment", size = bytes.len()))]
	pub async fn seal_chat_attachment(
		&self,
		room_id: &str,
		bytes: Vec<u8>,
	) -> Result<latest::ChatResource, ChatStageRejection> {
		if bytes.is_empty() || bytes.len() > CHAT_SEALED_ATTACHMENT_MAX_BYTES {
			return Err(ChatStageRejection::TooLarge {
				max_bytes: CHAT_SEALED_ATTACHMENT_MAX_BYTES as u32,
			});
		}
		let room_id = normalize_chat_identifier("roomId", room_id)
			.map_err(|_| ChatStageRejection::NotDeclared)?;
		if !self.room_staging(&room_id).await.sealed {
			return Err(ChatStageRejection::NotDeclared);
		}
		self.connection
			.require_remote_permission(latest::RemotePermission::PreimageSubmit, ())
			.await
			.map_err(|error| match error {
				truapi::CallError::HostFailure { reason } => {
					ChatStageRejection::SubmitFailed { reason }
				},
				_ => ChatStageRejection::PermissionDenied,
			})?;
		let Some(session) = self.accounts.current_session() else {
			return Err(ChatStageRejection::AllowanceUnavailable {
				reason: "No active session".to_string(),
			});
		};
		let part_bytes =
			if self.submits_preimages_locally() {
				CHAT_SEALED_LOCAL_PART_BYTES
			} else {
				let deadline = Instant::now() + PREIMAGE_SUBMIT_TIMEOUT;
				self.connection.services.bulletin.max_transaction_size(deadline).await.map_err(
					|error| ChatStageRejection::SubmitFailed { reason: error.to_string() },
				)? as usize
			};
		let sealed = SealedBytes::seal(&bytes, part_bytes)
			.map_err(|reason| ChatStageRejection::SubmitFailed { reason: reason.to_string() })?;
		let mut parts = Vec::with_capacity(sealed.parts.len());
		for part in sealed.parts {
			let deadline = Instant::now() + PREIMAGE_SUBMIT_TIMEOUT;
			let key = self
				.store_product_preimage(&CallContext::default(), &session, part, deadline)
				.await
				.map_err(|error| match error {
					ProductPreimageError::Allowance(reason) => {
						ChatStageRejection::AllowanceUnavailable { reason }
					},
					ProductPreimageError::Submit(reason) => {
						ChatStageRejection::SubmitFailed { reason }
					},
				})?;
			parts.push(key);
		}
		tracing::debug!(%room_id, parts = parts.len(), "sealed a chat attachment");
		Ok(latest::ChatResource::Sealed {
			parts,
			key: sealed.key.to_vec(),
			nonce: sealed.nonce.to_vec(),
			size: sealed.size as u64,
			digest: sealed.digest.to_vec(),
		})
	}

	/// Read the parts of a sealed resource, check them against their keys and
	/// the digest, and decrypt them. A part that the core stored or read
	/// before comes from its cache.
	#[instrument(skip_all, fields(runtime.method = "chat.open_sealed"))]
	pub async fn open_sealed_chat_resource(
		&self,
		resource: &latest::ChatResource,
	) -> Result<Vec<u8>, ChatOpenRejection> {
		let latest::ChatResource::Sealed { parts, key, nonce, size, digest } = resource else {
			return Err(ChatOpenRejection::Invalid { reason: "not sealed".into() });
		};
		let invalid = |reason: &str| ChatOpenRejection::Invalid { reason: reason.into() };
		if parts.is_empty() || parts.len() > CHAT_SEALED_PARTS_MAX {
			return Err(invalid("parts"));
		}
		let mut ciphertext = Vec::new();
		for (index, part) in parts.iter().enumerate() {
			let part_key =
				<[u8; 32]>::try_from(part.as_slice()).map_err(|_| invalid("part key"))?;
			let value = self
				.read_sealed_part(part_key)
				.await
				.ok_or(ChatOpenRejection::Missing { index: index as u32 })?;
			ciphertext.extend_from_slice(&value);
			if ciphertext.len() as u64 > *size {
				return Err(ChatOpenRejection::Mismatch);
			}
		}
		SealedBytes::open(&ciphertext, key, nonce, *size, digest)
	}

	/// One part of a sealed resource: from the cache, else from the host
	/// content backend. A value that does not hash to `key` is no part.
	async fn read_sealed_part(&self, key: [u8; 32]) -> Option<Vec<u8>> {
		if let Some(value) = self.connection.services.cached_preimage(&key) {
			return Some(value);
		}
		let lookups =
			self.connection
				.platform
				.lookup_preimage(key.to_vec())
				.filter_map(|item| async move {
					item.ok().flatten().filter(|value| preimage_key(value) == key)
				});
		futures::pin_mut!(lookups);
		let wait = futures_timer::Delay::new(SEALED_PART_WAIT);
		let value = match futures::future::select(lookups.next(), wait).await {
			futures::future::Either::Left((Some(value), _)) => value,
			_ => return None,
		};
		self.connection.services.cache_preimage(key, value.clone());
		Some(value)
	}
}

/// The part size of a sealed attachment on a host that keeps its preimages
/// in the core.
const CHAT_SEALED_LOCAL_PART_BYTES: usize = 2 * 1024 * 1024;

/// The ciphertext of a sealed attachment, cut into parts, and what opens it.
struct SealedBytes {
	parts: Vec<Vec<u8>>,
	key: [u8; 32],
	nonce: [u8; 24],
	size: usize,
	digest: [u8; 64],
}

impl SealedBytes {
	/// Encrypt `bytes` with a new key and nonce, and cut the ciphertext into
	/// parts of at most `part_bytes`.
	fn seal(bytes: &[u8], part_bytes: usize) -> Result<Self, &'static str> {
		if part_bytes == 0 {
			return Err("the Bulletin chain takes no bytes");
		}
		let mut key = [0u8; 32];
		let mut nonce = [0u8; 24];
		getrandom::getrandom(&mut key).map_err(|_| "no random key")?;
		getrandom::getrandom(&mut nonce).map_err(|_| "no random nonce")?;
		let ciphertext = XChaCha20Poly1305::new((&key).into())
			.encrypt(XNonce::from_slice(&nonce), Payload { msg: bytes, aad: CHAT_SEALED_AAD })
			.map_err(|_| "the encryption failed")?;
		let parts: Vec<Vec<u8>> = ciphertext.chunks(part_bytes).map(<[u8]>::to_vec).collect();
		if parts.len() > CHAT_SEALED_PARTS_MAX {
			return Err("the attachment needs too many parts");
		}
		Ok(Self {
			parts,
			key,
			nonce,
			size: ciphertext.len(),
			digest: Sha512::digest(&ciphertext).into(),
		})
	}

	/// Check `ciphertext` against `size` and `digest`, and decrypt it.
	fn open(
		ciphertext: &[u8],
		key: &[u8],
		nonce: &[u8],
		size: u64,
		digest: &[u8],
	) -> Result<Vec<u8>, ChatOpenRejection> {
		if ciphertext.len() as u64 != size || Sha512::digest(ciphertext).as_slice() != digest {
			return Err(ChatOpenRejection::Mismatch);
		}
		let key = <[u8; 32]>::try_from(key).map_err(|_| ChatOpenRejection::Mismatch)?;
		if nonce.len() != 24 {
			return Err(ChatOpenRejection::Mismatch);
		}
		XChaCha20Poly1305::new((&key).into())
			.decrypt(XNonce::from_slice(nonce), Payload { msg: ciphertext, aad: CHAT_SEALED_AAD })
			.map_err(|_| ChatOpenRejection::Mismatch)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn a_sealed_attachment_opens_from_its_parts_in_order() {
		let bytes: Vec<u8> = (0..5000u32).map(|n| (n % 251) as u8).collect();
		let sealed = SealedBytes::seal(&bytes, 1024).unwrap();
		assert_eq!(sealed.parts.len(), 5);
		assert_eq!(sealed.size, bytes.len() + crate::platform::CHAT_SEALED_TAG_BYTES);
		let ciphertext = sealed.parts.concat();
		assert_eq!(
			SealedBytes::open(
				&ciphertext,
				&sealed.key,
				&sealed.nonce,
				sealed.size as u64,
				&sealed.digest
			),
			Ok(bytes)
		);
	}

	#[test]
	fn a_sealed_attachment_with_a_changed_part_or_key_does_not_open() {
		let sealed = SealedBytes::seal(b"a private photo", 8).unwrap();
		let size = sealed.size as u64;
		let mut swapped = sealed.parts.clone();
		swapped.swap(0, 1);
		assert_eq!(
			SealedBytes::open(&swapped.concat(), &sealed.key, &sealed.nonce, size, &sealed.digest),
			Err(ChatOpenRejection::Mismatch)
		);
		let mut other_key = sealed.key;
		other_key[0] ^= 1;
		assert_eq!(
			SealedBytes::open(
				&sealed.parts.concat(),
				&other_key,
				&sealed.nonce,
				size,
				&sealed.digest
			),
			Err(ChatOpenRejection::Mismatch)
		);
	}
}
