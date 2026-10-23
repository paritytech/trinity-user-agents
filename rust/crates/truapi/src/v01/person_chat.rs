use parity_scale_codec::{Decode, Encode};

use super::{Statement, StatementProof};

/// Which device of the identity this host is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum PersonChatRole {
	/// The wallet device holding the identity entropy.
	Primary,
	/// A device paired with the wallet, signing with its own statement key.
	Linked,
}

/// Another device of the same identity, addressed in device fan-out.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct PersonChatDevice {
	/// Account that signs the device's statements.
	pub statement_account_id: [u8; 32],
	/// X25519 public key the device decrypts with.
	pub enc_public_key: [u8; 32],
	/// Name the user knows the device by.
	pub name: String,
}

/// Request for the chat profile of the signed-in identity.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPersonChatGetProfileRequest {}

/// The chat profile of the signed-in identity on this device.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPersonChatGetProfileResponse {
	/// Identity account, as peers look it up on Asset Hub.
	pub identity_account_id: [u8; 32],
	/// X25519 public key addressing the identity in chat.
	pub chat_public_key: [u8; 32],
	/// Short username, when the identity has one.
	pub lite_username: Option<String>,
	/// Full username, when the identity has one.
	pub full_username: Option<String>,
	/// Which device of the identity this host is.
	pub role: PersonChatRole,
	/// Account that `proveStatement` signs with: the identity account on the
	/// primary device, the device statement account on a linked one.
	pub statement_account_id: [u8; 32],
	/// This device's X25519 public key.
	pub device_enc_public_key: [u8; 32],
	/// Every device of the identity that peers address, this one included.
	pub own_devices: Vec<PersonChatDevice>,
}

/// Request for the X25519 secrets of the chat protocol.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPersonChatSecretsRequest {}

/// The X25519 secrets the chat product agrees per-peer keys with.
#[derive(Clone, PartialEq, Eq, Encode, Decode, derive_more::Debug)]
pub struct HostPersonChatSecretsResponse {
	/// Secret of the identity's chat key.
	#[debug("<redacted>")]
	pub chat_identity_secret: [u8; 32],
	/// Secret of this device's encryption key.
	#[debug("<redacted>")]
	pub device_enc_secret: [u8; 32],
}

/// Request to prove an unsigned statement with the profile's statement account.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPersonChatProveStatementRequest {
	/// The statement, without a proof.
	pub statement: Statement,
}

/// Proof of a statement.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPersonChatProveStatementResponse {
	/// Sr25519 proof by the profile's statement account.
	pub proof: StatementProof,
}

/// Request to sign a chat request as the identity account.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPersonChatProveChatRequestRequest {
	/// SCALE encoding of the chat request message.
	pub message: Vec<u8>,
	/// Account of the person asked to accept.
	pub acceptor: [u8; 32],
}

/// Identity signature of a chat request.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPersonChatProveChatRequestResponse {
	/// Sr25519 signature over the message followed by the SCALE-encoded acceptor.
	pub signature: [u8; 64],
}

/// Why a `PersonChat` call failed.
///
/// A product other than the chat product, or a host that cannot serve the
/// call, gets `Unsupported` at the framework level instead.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostPersonChatError {
	/// No identity is signed in.
	NotConnected,
	/// The request has a shape the call refuses.
	InvalidRequest {
		/// What is wrong with it.
		reason: String,
	},
	/// Catch-all.
	Unknown {
		/// Human-readable reason.
		reason: String,
	},
}

/// The push platform of a device, as chat `Token` messages name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum PersonChatPushPlatform {
	/// Firebase Cloud Messaging.
	Android,
	/// Apple Push Notification service.
	Ios,
}

/// One push token of this device, as chat `Token` messages carry it.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct PersonChatPushToken {
	/// The token bytes: the APNs device token, or the UTF-8 of the FCM token.
	pub token: Vec<u8>,
	/// The platform of the token.
	pub platform: PersonChatPushPlatform,
}

/// Request for the push tokens of this device.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPersonChatPushTokensRequest {}

/// The push tokens of this device, which the chat product shares with its contacts.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPersonChatPushTokensResponse {
	/// The push token, when the device has one now.
	pub push: Option<PersonChatPushToken>,
	/// The PushKit VoIP token bytes, on iOS when it has one.
	pub voip: Option<Vec<u8>>,
}

/// Request to notify a contact's device through the push relay.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPersonChatNotifyPeerRequest {
	/// The contact's device token, as the relay takes it: the hex of an APNs
	/// or VoIP token, or the FCM token.
	pub device_token: String,
	/// The id under which the contact's device finds the chat.
	pub push_id: [u8; 32],
	/// The platform of the contact's device.
	pub platform: PersonChatPushPlatform,
	/// The notification payload, already encrypted for the contact.
	pub message: Vec<u8>,
	/// Whether the push goes out as a VoIP push.
	pub voip: bool,
}

/// Whether the push relay took a push.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPersonChatNotifyPeerResponse {
	/// True when the push service took it.
	pub delivered: bool,
}

/// How the host shows the pushes of one contact while no worker runs.
#[derive(Clone, PartialEq, Eq, Encode, Decode, derive_more::Debug)]
pub struct PersonChatPushRoute {
	/// The push id the contact sends to this device under.
	pub push_id: [u8; 32],
	/// The room of the chat product the notification opens.
	pub room_id: String,
	/// The title of the notification: the contact's name.
	pub title: String,
	/// The ChaCha20-Poly1305 key of the contact's payloads.
	#[debug("<redacted>")]
	pub key: [u8; 32],
}

/// Request to replace the chat push routes of this device.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPersonChatSetPushRoutesRequest {
	/// Every route; the host forgets routes not listed.
	pub routes: Vec<PersonChatPushRoute>,
}

/// Request to upload a file to a HOP node, for a contact to claim.
#[derive(Clone, PartialEq, Eq, Encode, Decode, derive_more::Debug)]
pub struct HostPersonChatHopUploadRequest {
	/// The file.
	#[debug("{} bytes", data.len())]
	pub data: Vec<u8>,
}

/// Where a contact claims an uploaded file, as a chat message carries it.
#[derive(Clone, PartialEq, Eq, Encode, Decode, derive_more::Debug)]
pub struct HostPersonChatHopUploadResponse {
	/// The identifier of the file's root entry.
	pub identifier: [u8; 32],
	/// The ticket that opens and claims the file.
	#[debug("<redacted>")]
	pub ticket: [u8; 32],
	/// The `wss://` URL of the node that holds it.
	pub node: String,
}

/// Request to download a file a contact uploaded.
#[derive(Clone, PartialEq, Eq, Encode, Decode, derive_more::Debug)]
pub struct HostPersonChatHopDownloadRequest {
	/// The identifier of the file's root entry.
	pub identifier: [u8; 32],
	/// The ticket the contact sent.
	#[debug("<redacted>")]
	pub ticket: [u8; 32],
	/// The node the contact uploaded to; the host refuses a node it does not trust.
	pub node: String,
}

/// A downloaded file.
#[derive(Clone, PartialEq, Eq, Encode, Decode, derive_more::Debug)]
pub struct HostPersonChatHopDownloadResponse {
	/// The file.
	#[debug("{} bytes", data.len())]
	pub data: Vec<u8>,
	/// The entries the node still holds for it, to ack once the file is kept.
	pub entries: Vec<[u8; 32]>,
}

/// Request to tell a node that the entries of a file were taken.
#[derive(Clone, PartialEq, Eq, Encode, Decode, derive_more::Debug)]
pub struct HostPersonChatHopAckRequest {
	/// The node the file came from.
	pub node: String,
	/// The ticket of the file.
	#[debug("<redacted>")]
	pub ticket: [u8; 32],
	/// The entries a download returned.
	pub entries: Vec<[u8; 32]>,
}

/// Request to prepare the delivery of a chat request.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPersonChatPrepareDeliveryRequest {
	/// The id of the chat request, which names its delivery account.
	pub request_id: String,
	/// The size of the request statement's data, in bytes.
	pub size: u32,
}

/// What delivers a chat request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum PersonChatDelivery {
	/// The delivery account of `period`, which a notification slot lets post
	/// the request; prove the request statement with `proveDelivery`.
	Anonymous {
		/// The notification period.
		period: u32,
		/// The delivery account.
		account_id: [u8; 32],
	},
	/// No notification slot is free: prove the request with `proveStatement`.
	NoSlot,
}

/// The delivery of a chat request.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPersonChatPrepareDeliveryResponse {
	/// What delivers the request.
	pub delivery: PersonChatDelivery,
}

/// Request to prove a chat request statement with its delivery account.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPersonChatProveDeliveryRequest {
	/// The id of the chat request.
	pub request_id: String,
	/// The period `prepareDelivery` gave.
	pub period: u32,
	/// The unsigned statement.
	pub statement: Statement,
}

/// The proof of a chat request statement.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPersonChatProveDeliveryResponse {
	/// The proof, by the delivery account.
	pub proof: StatementProof,
}

/// A contact of the chat, for the parts of the app outside the chat.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct PersonChatContact {
	/// The contact's identity account.
	pub account_id: [u8; 32],
	/// The contact's username, when the chat knows it.
	pub username: Option<String>,
	/// The room of the chat product that holds the chat with the contact.
	pub room_id: String,
	/// Whether the user blocked the contact.
	pub blocked: bool,
}

/// Request to replace the contacts the host keeps.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPersonChatPublishContactsRequest {
	/// Every contact; the host forgets contacts not listed.
	pub contacts: Vec<PersonChatContact>,
}

/// Request for a part of the native chat history the host staged for the
/// chat product when it moved its native chat over.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPersonChatHistoryExportRequest {
	/// Where the part starts, in bytes.
	pub offset: u64,
}

/// A part of the staged native chat history.
#[derive(Clone, PartialEq, Eq, Encode, Decode, derive_more::Debug)]
pub struct HostPersonChatHistoryExportResponse {
	/// The size of the whole export in bytes; 0 when the host staged none.
	pub total: u64,
	/// The bytes from the requested offset, at most 1 MiB.
	#[debug("{} bytes", data.len())]
	pub data: Vec<u8>,
}

/// Request to tell the host that the staged history is imported.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostPersonChatHistoryImportedRequest {}

/// Request to hand the host the coins of a payment the contact sent, for it
/// to claim with its own coinage flow.
#[derive(Clone, PartialEq, Eq, Encode, Decode, derive_more::Debug)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct HostPersonChatPaymentReceivedRequest {
	/// The room of the chat product that holds the payment message.
	pub room_id: String,
	/// The id of the payment message: the id the host keeps the claim under.
	pub message_id: String,
	/// Milliseconds since the Unix epoch when the contact paid.
	pub timestamp: u64,
	/// The sum of the coins the message names, in the smallest unit.
	pub total_value: u128,
	/// The 64-byte sr25519 secret key of each coin.
	#[debug("{} keys", coin_keys.len())]
	pub coin_keys: Vec<Vec<u8>>,
}
