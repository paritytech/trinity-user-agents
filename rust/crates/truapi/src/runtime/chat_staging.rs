//! Staging of chat attachments.
//!
//! A user attachment above the inline bound goes to the Bulletin chain as one
//! preimage, and the message carries its key. The product's own Bulletin
//! allowance pays for it, under the product's `PreimageSubmit` permission, and
//! only in a room whose features declare `preimage_attachments`. The host asks
//! the core to stage an attachment. Neither the host nor the product holds the
//! allowance key.
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

use super::{PREIMAGE_SUBMIT_TIMEOUT, ProductPreimageError, ProductRuntimeHost};
use crate::platform::{CHAT_PREIMAGE_ATTACHMENT_MAX_BYTES, normalize_chat_identifier};

/// Why the core did not stage a chat attachment.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display, derive_more::Error)]
pub enum ChatStageRejection {
	/// The room does not declare `preimage_attachments`.
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

/// The `preimage_attachments` flag of each room, as the core last saw the
/// room's features, by product and room. Shared by every connection of one
/// host runtime.
#[derive(Default)]
pub struct ChatStagingDeclarations {
	rooms: Mutex<HashMap<(String, String), bool>>,
}

impl ChatStagingDeclarations {
	/// Keep the flag that the features of `room_id` declare now.
	pub fn record(&self, product_id: &str, room_id: &str, declared: bool) {
		self.lock().insert((product_id.to_string(), room_id.to_string()), declared);
	}

	/// Forget a room that is gone.
	pub fn forget(&self, product_id: &str, room_id: &str) {
		self.lock().remove(&(product_id.to_string(), room_id.to_string()));
	}

	/// The flag the core last saw, or `None` when it saw none.
	fn declared(&self, product_id: &str, room_id: &str) -> Option<bool> {
		self.lock().get(&(product_id.to_string(), room_id.to_string())).copied()
	}

	fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<(String, String), bool>> {
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
		if !self.room_stages_attachments(&room_id).await {
			return Err(ChatStageRejection::NotDeclared);
		}
		self.require_remote_permission(latest::RemotePermission::PreimageSubmit, ())
			.await
			.map_err(|error| match error {
				truapi::CallError::HostFailure { reason } => {
					ChatStageRejection::SubmitFailed { reason }
				},
				_ => ChatStageRejection::PermissionDenied,
			})?;
		let Some(session) = self.authority.current_session() else {
			return Err(ChatStageRejection::AllowanceUnavailable {
				reason: "No active session".to_string(),
			});
		};
		let submission_deadline = Instant::now() + PREIMAGE_SUBMIT_TIMEOUT;
		if !self.submits_preimages_locally() {
			let bound =
				self.services.bulletin.max_transaction_size(submission_deadline).await.map_err(
					|error| ChatStageRejection::SubmitFailed { reason: error.to_string() },
				)?;
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

	/// Keep the `preimage_attachments` flag of features the core accepted for
	/// `room_id`.
	pub fn record_chat_staging(&self, room_id: &str, features: &latest::ChatRoomFeatures) {
		self.services.chat_staging.record(
			&self.product.product_id,
			room_id,
			features.preimage_attachments,
		);
	}

	/// Forget the staging flag of a deleted room.
	pub fn forget_chat_staging(&self, room_id: &str) {
		self.services.chat_staging.forget(&self.product.product_id, room_id);
	}

	/// Whether `room_id` declares `preimage_attachments`: as the core last
	/// saw it, or else as the chat adapter of this connection reads it from
	/// the host.
	async fn room_stages_attachments(&self, room_id: &str) -> bool {
		if let Some(declared) =
			self.services.chat_staging.declared(&self.product.product_id, room_id)
		{
			return declared;
		}
		let Some(platform) = self.chat_platform.as_ref() else { return false };
		match platform.read_chat_room_features(&self.product, room_id.to_string()).await {
			Ok(features) => {
				self.record_chat_staging(room_id, &features);
				features.preimage_attachments
			},
			Err(error) => {
				warn!(reason = %error.reason, "chat room features could not be read");
				false
			},
		}
	}
}
