//! Unified [`PersonChat`] trait.

use crate::{
	CallContext, CallError,
	versioned::person_chat::{
		HostPersonChatError, HostPersonChatGetProfileRequest, HostPersonChatGetProfileResponse,
		HostPersonChatHistoryExportRequest, HostPersonChatHistoryExportResponse,
		HostPersonChatHistoryImportedRequest, HostPersonChatHistoryImportedResponse,
		HostPersonChatHopAckRequest, HostPersonChatHopAckResponse,
		HostPersonChatHopDownloadRequest, HostPersonChatHopDownloadResponse,
		HostPersonChatHopUploadRequest, HostPersonChatHopUploadResponse,
		HostPersonChatNotifyPeerRequest, HostPersonChatNotifyPeerResponse,
		HostPersonChatPrepareDeliveryRequest, HostPersonChatPrepareDeliveryResponse,
		HostPersonChatProveChatRequestRequest, HostPersonChatProveChatRequestResponse,
		HostPersonChatProveDeliveryRequest, HostPersonChatProveDeliveryResponse,
		HostPersonChatProveStatementRequest, HostPersonChatProveStatementResponse,
		HostPersonChatPublishContactsRequest, HostPersonChatPublishContactsResponse,
		HostPersonChatPushTokensRequest, HostPersonChatPushTokensResponse,
		HostPersonChatSecretsRequest, HostPersonChatSecretsResponse,
		HostPersonChatSetPushRoutesRequest, HostPersonChatSetPushRoutesResponse,
	},
	wire, wire_trait,
};

/// The keys of the person-to-person chat protocol, for the chat product.
///
/// Served only to the chat product (the `chat` label on any dotNS network):
/// any other product, or a host without the identity's keys, gets
/// `Unsupported`. The identity's sr25519 keys never leave the host; the X25519
/// secrets do, because the chat product agrees per-peer keys itself.
#[wire_trait(id = 30)]
#[crate::service(required_execution = Worker)]
#[crate::async_trait]
pub trait PersonChat: Send + Sync {
	/// The chat profile of the signed-in identity on this device.
	///
	/// ```ts
	/// const result = await truapi.personChat.getProfile({});
	/// if (result.isErr()) {
	///   console.log("getProfile unavailable:", result.error);
	/// } else {
	///   console.log("chat profile:", result.value);
	/// }
	/// ```
	#[wire(id = 0)]
	async fn get_profile(
		&self,
		_cx: &CallContext,
		_request: HostPersonChatGetProfileRequest,
	) -> Result<HostPersonChatGetProfileResponse, CallError<HostPersonChatError>> {
		Err(CallError::unavailable())
	}

	/// The X25519 secrets of the identity's chat key and this device's key.
	///
	/// ```ts
	/// const result = await truapi.personChat.chatSecrets({});
	/// console.log("chatSecrets ok:", result.isOk());
	/// ```
	#[wire(id = 1)]
	async fn chat_secrets(
		&self,
		_cx: &CallContext,
		_request: HostPersonChatSecretsRequest,
	) -> Result<HostPersonChatSecretsResponse, CallError<HostPersonChatError>> {
		Err(CallError::unavailable())
	}

	/// Prove an unsigned statement with the profile's statement account.
	///
	/// ```ts
	/// const expiry = BigInt(Math.floor(Date.now() / 1000) + 86400) << 32n;
	/// const bytes = crypto.getRandomValues(new Uint8Array(32));
	/// const topic: `0x${string}` = `0x${bytes.toHex()}`;
	/// const result = await truapi.personChat.proveStatement({
	///   statement: { expiry, topics: [topic] },
	/// });
	/// console.log("proveStatement ok:", result.isOk());
	/// ```
	#[wire(id = 2)]
	async fn prove_statement(
		&self,
		_cx: &CallContext,
		_request: HostPersonChatProveStatementRequest,
	) -> Result<HostPersonChatProveStatementResponse, CallError<HostPersonChatError>> {
		Err(CallError::unavailable())
	}

	/// Sign a chat request as the identity account.
	///
	/// The host signs the message followed by the SCALE-encoded acceptor, and
	/// refuses a message that is not a chat request message.
	///
	/// ```ts
	/// const result = await truapi.personChat.proveChatRequest({
	///   message: "0x00",
	///   acceptor: `0x${"00".repeat(32)}`,
	/// });
	/// console.log("proveChatRequest ok:", result.isOk());
	/// ```
	#[wire(id = 3)]
	async fn prove_chat_request(
		&self,
		_cx: &CallContext,
		_request: HostPersonChatProveChatRequestRequest,
	) -> Result<HostPersonChatProveChatRequestResponse, CallError<HostPersonChatError>> {
		Err(CallError::unavailable())
	}

	/// The push tokens of this device, for the chat product's `Token` messages.
	///
	/// ```ts
	/// const result = await truapi.personChat.pushTokens({});
	/// console.log("pushTokens ok:", result.isOk());
	/// ```
	#[wire(id = 4)]
	async fn push_tokens(
		&self,
		_cx: &CallContext,
		_request: HostPersonChatPushTokensRequest,
	) -> Result<HostPersonChatPushTokensResponse, CallError<HostPersonChatError>> {
		Err(CallError::unavailable())
	}

	/// Notify a contact's device through the host's push relay. The message is
	/// already encrypted for the contact; the host only relays it.
	///
	/// ```ts
	/// const result = await truapi.personChat.notifyPeer({
	///   deviceToken: "00",
	///   pushId: `0x${"00".repeat(32)}`,
	///   platform: "Ios",
	///   message: "0x00",
	///   voip: false,
	/// });
	/// console.log("notifyPeer ok:", result.isOk());
	/// ```
	#[wire(id = 5)]
	async fn notify_peer(
		&self,
		_cx: &CallContext,
		_request: HostPersonChatNotifyPeerRequest,
	) -> Result<HostPersonChatNotifyPeerResponse, CallError<HostPersonChatError>> {
		Err(CallError::unavailable())
	}

	/// Replace the routes the host uses to show the chat pushes of each
	/// contact while no worker runs.
	///
	/// ```ts
	/// const result = await truapi.personChat.setPushRoutes({ routes: [] });
	/// console.log("setPushRoutes ok:", result.isOk());
	/// ```
	#[wire(id = 6)]
	async fn set_push_routes(
		&self,
		_cx: &CallContext,
		_request: HostPersonChatSetPushRoutesRequest,
	) -> Result<HostPersonChatSetPushRoutesResponse, CallError<HostPersonChatError>> {
		Err(CallError::unavailable())
	}

	/// Upload a file to one of the host's HOP nodes for a contact to claim.
	///
	/// The host makes the ticket, seals the file as the native apps do, and
	/// signs the submit with the product's Bulletin allowance account, which
	/// pays if the contact never claims it and the node keeps it on chain.
	///
	/// ```ts
	/// const result = await truapi.personChat.hopUpload({ data: "0x00" });
	/// console.log("hopUpload ok:", result.isOk());
	/// ```
	#[wire(id = 7)]
	async fn hop_upload(
		&self,
		_cx: &CallContext,
		_request: HostPersonChatHopUploadRequest,
	) -> Result<HostPersonChatHopUploadResponse, CallError<HostPersonChatError>> {
		Err(CallError::unavailable())
	}

	/// Download and open a file a contact uploaded, from the pool or, once
	/// promoted, from Bulletin.
	///
	/// ```ts
	/// const result = await truapi.personChat.hopDownload({
	///   identifier: `0x${"00".repeat(32)}`,
	///   ticket: `0x${"00".repeat(32)}`,
	///   node: "wss://hop.example",
	/// });
	/// console.log("hopDownload ok:", result.isOk());
	/// ```
	#[wire(id = 8)]
	async fn hop_download(
		&self,
		_cx: &CallContext,
		_request: HostPersonChatHopDownloadRequest,
	) -> Result<HostPersonChatHopDownloadResponse, CallError<HostPersonChatError>> {
		Err(CallError::unavailable())
	}

	/// Tell the node that the entries of a downloaded file were taken, so it
	/// drops them.
	///
	/// ```ts
	/// const result = await truapi.personChat.hopAck({
	///   node: "wss://hop.example",
	///   ticket: `0x${"00".repeat(32)}`,
	///   entries: [],
	/// });
	/// console.log("hopAck ok:", result.isOk());
	/// ```
	#[wire(id = 9)]
	async fn hop_ack(
		&self,
		_cx: &CallContext,
		_request: HostPersonChatHopAckRequest,
	) -> Result<HostPersonChatHopAckResponse, CallError<HostPersonChatError>> {
		Err(CallError::unavailable())
	}

	/// Prepare the delivery of a chat request as the native apps deliver one:
	/// the delivery account of the current notification period, with a
	/// notification slot claimed for it so the statement store takes the
	/// request from it, or `NoSlot` when no slot is free. A slot that does not
	/// take effect within five minutes fails the call; try again later.
	///
	/// ```ts
	/// const result = await truapi.personChat.prepareDelivery({ requestId: "r1", size: 100 });
	/// console.log("prepareDelivery ok:", result.isOk());
	/// ```
	#[wire(id = 10)]
	async fn prepare_delivery(
		&self,
		_cx: &CallContext,
		_request: HostPersonChatPrepareDeliveryRequest,
	) -> Result<HostPersonChatPrepareDeliveryResponse, CallError<HostPersonChatError>> {
		Err(CallError::unavailable())
	}

	/// Prove a chat request statement with the delivery account that
	/// `prepareDelivery` gave for it.
	///
	/// ```ts
	/// const expiry = BigInt(Math.floor(Date.now() / 1000) + 86400) << 32n;
	/// const result = await truapi.personChat.proveDelivery({
	///   requestId: "r1",
	///   period: 0,
	///   statement: { expiry, topics: [] },
	/// });
	/// console.log("proveDelivery ok:", result.isOk());
	/// ```
	#[wire(id = 11)]
	async fn prove_delivery(
		&self,
		_cx: &CallContext,
		_request: HostPersonChatProveDeliveryRequest,
	) -> Result<HostPersonChatProveDeliveryResponse, CallError<HostPersonChatError>> {
		Err(CallError::unavailable())
	}

	/// Replace the contacts the host keeps for the parts of the app outside
	/// the chat: the wallet, caller names and the contact picker of other
	/// products.
	///
	/// ```ts
	/// const result = await truapi.personChat.publishContacts({ contacts: [] });
	/// console.log("publishContacts ok:", result.isOk());
	/// ```
	#[wire(id = 12)]
	async fn publish_contacts(
		&self,
		_cx: &CallContext,
		_request: HostPersonChatPublishContactsRequest,
	) -> Result<HostPersonChatPublishContactsResponse, CallError<HostPersonChatError>> {
		Err(CallError::unavailable())
	}

	/// A part of the native chat history the host staged for the chat product
	/// when it moved the native chat of this install over. `total` is 0 when
	/// there is none. Read from offset 0 until `total`, then call
	/// `historyImported`.
	///
	/// ```ts
	/// const result = await truapi.personChat.historyExport({ offset: 0n });
	/// console.log("historyExport ok:", result.isOk());
	/// ```
	#[wire(id = 13)]
	async fn history_export(
		&self,
		_cx: &CallContext,
		_request: HostPersonChatHistoryExportRequest,
	) -> Result<HostPersonChatHistoryExportResponse, CallError<HostPersonChatError>> {
		Err(CallError::unavailable())
	}

	/// Tell the host that the staged history is imported, so it may drop it
	/// and the native chat's tables.
	///
	/// ```ts
	/// const result = await truapi.personChat.historyImported({});
	/// console.log("historyImported ok:", result.isOk());
	/// ```
	#[wire(id = 14)]
	async fn history_imported(
		&self,
		_cx: &CallContext,
		_request: HostPersonChatHistoryImportedRequest,
	) -> Result<HostPersonChatHistoryImportedResponse, CallError<HostPersonChatError>> {
		Err(CallError::unavailable())
	}
}
