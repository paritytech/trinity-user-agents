//! Staging of chat attachments.
//!
//! A user photo, video or file of a room of the chat product goes to a HOP
//! node of the person-to-person chat, and the message carries the
//! `ChatResource::Hop` that names it, only in a room whose features declare
//! `hop_attachments`. The host asks the core to upload an attachment.
//!
//! The core reads the declaration of the room from what it saw itself: each
//! `set_room_features` it accepted. When it saw none since the runtime
//! started, it reads the features the host keeps through the chat adapter of
//! the product. With neither, the room declares nothing.

use std::{
	collections::HashMap,
	sync::{Mutex, PoisonError},
};

use tracing::{instrument, warn};
use truapi::{CallContext, latest};

use super::ProductRuntimeHost;
use crate::platform::{CHAT_HOP_FILE_MAX_BYTES, normalize_chat_identifier};

/// Why the core did not stage a chat attachment.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display, derive_more::Error)]
pub enum ChatStageRejection {
	/// The room does not declare `hop_attachments`, or the product is not the
	/// chat product.
	#[display("the room does not declare HOP attachments")]
	NotDeclared,
	/// The attachment is larger than the core stages.
	#[display("the attachment is larger than {max_bytes} bytes")]
	TooLarge {
		/// The largest attachment the core stages, in bytes.
		max_bytes: u32,
	},
	/// The upload failed.
	#[display("the upload failed: {reason}")]
	SubmitFailed {
		/// Human-readable reason.
		reason: String,
	},
}

/// Whether each room declares `hop_attachments`, as the core last saw the
/// room's features, by product and room. Shared by every connection of one
/// host runtime.
#[derive(Default)]
pub struct ChatStagingDeclarations {
	rooms: Mutex<HashMap<(String, String), bool>>,
}

impl ChatStagingDeclarations {
	/// Keep what `features` of `room_id` declare now.
	pub fn record(&self, product_id: &str, room_id: &str, features: &latest::ChatRoomFeatures) {
		self.lock()
			.insert((product_id.to_string(), room_id.to_string()), features.hop_attachments);
	}

	/// Forget a room that is gone.
	pub fn forget(&self, product_id: &str, room_id: &str) {
		self.lock().remove(&(product_id.to_string(), room_id.to_string()));
	}

	/// What the core last saw, or `None` when it saw nothing.
	fn declared(&self, product_id: &str, room_id: &str) -> Option<bool> {
		self.lock().get(&(product_id.to_string(), room_id.to_string())).copied()
	}

	fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<(String, String), bool>> {
		self.rooms.lock().unwrap_or_else(PoisonError::into_inner)
	}
}

impl ProductRuntimeHost {
	/// Keep the attachment declaration of features the core accepted for
	/// `room_id`.
	pub fn record_chat_staging(&self, room_id: &str, features: &latest::ChatRoomFeatures) {
		self.connection.services.chat_staging.record(
			&self.connection.product.product_id,
			room_id,
			features,
		);
	}

	/// Forget the attachment declaration of a deleted room.
	pub fn forget_chat_staging(&self, room_id: &str) {
		self.connection
			.services
			.chat_staging
			.forget(&self.connection.product.product_id, room_id);
	}

	/// Whether `room_id` declares `hop_attachments`: as the core last saw it,
	/// or else as the chat adapter of this connection reads it from the host.
	async fn room_declares_hop(&self, room_id: &str) -> bool {
		if let Some(declared) = self
			.connection
			.services
			.chat_staging
			.declared(&self.connection.product.product_id, room_id)
		{
			return declared;
		}
		let Some(platform) = self.connection.chat_platform.as_ref() else {
			return false;
		};
		match platform
			.read_chat_room_features(&self.connection.product, room_id.to_string())
			.await
		{
			Ok(features) => {
				self.record_chat_staging(room_id, &features);
				features.hop_attachments
			},
			Err(error) => {
				warn!(reason = %error.reason, "chat room features could not be read");
				false
			},
		}
	}

	/// Whether the core uploads this product's attachments to HOP: the chat
	/// product, on a core that reaches HOP nodes and a host that lists them.
	pub fn backs_hop_attachments(&self) -> bool {
		self.connection.product.is_person_chat_product() &&
			self.connection.services.hop_connector().is_some() &&
			self.connection.services.person_chat_platform().is_some()
	}

	/// Upload `bytes`, a user photo, video or file of `room_id`, to one of the
	/// host's HOP nodes under a new ticket, signed with the product's
	/// Bulletin allowance account, and answer the `ChatResource::Hop` that
	/// names it.
	///
	/// The core refuses an attachment above [`CHAT_HOP_FILE_MAX_BYTES`], a
	/// room that does not declare `hop_attachments`, and any product but the
	/// chat product. `progress` gets the bytes the node holds so far and the
	/// size of the file, as [`HopClient::upload`] reports them.
	///
	/// [`HopClient::upload`]: crate::runtime::hop_client::HopClient::upload
	#[instrument(skip_all, fields(runtime.method = "chat.hop_attachment", size = bytes.len()))]
	pub async fn hop_chat_attachment(
		&self,
		room_id: &str,
		bytes: Vec<u8>,
		progress: impl FnMut(u64, u64) + Send,
	) -> Result<latest::ChatResource, ChatStageRejection> {
		if bytes.is_empty() || bytes.len() as u64 > CHAT_HOP_FILE_MAX_BYTES {
			return Err(ChatStageRejection::TooLarge { max_bytes: CHAT_HOP_FILE_MAX_BYTES as u32 });
		}
		if !self.backs_hop_attachments() {
			return Err(ChatStageRejection::NotDeclared);
		}
		let room_id = normalize_chat_identifier("roomId", room_id)
			.map_err(|_| ChatStageRejection::NotDeclared)?;
		if !self.room_declares_hop(&room_id).await {
			return Err(ChatStageRejection::NotDeclared);
		}
		let size = bytes.len() as u64;
		let uploaded = self
			.upload_hop_file(&CallContext::default(), bytes, progress)
			.await
			.map_err(|reason| ChatStageRejection::SubmitFailed { reason })?;
		tracing::debug!(%room_id, size, "uploaded a chat attachment to HOP");
		Ok(latest::ChatResource::Hop {
			identifier: uploaded.identifier.to_vec(),
			ticket: uploaded.ticket.to_vec(),
			node: uploaded.node,
			size,
		})
	}
}
