//! Product-facing PersonChat capability adapter.

use parity_scale_codec::{Compact, Decode, Encode};
use tracing::instrument;
use truapi::{
	CallContext, CallError,
	api::PersonChat,
	latest,
	versioned::{
		FromLatest, IntoLatest, Versioned,
		person_chat::{
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
	},
};

use super::from_latest_error;
use crate::{
	host_internal::bulletin::allowance_signer,
	host_logic::{
		session::SessionInfo,
		sso::pairing::x25519_public_key,
		statement_store::{
			StatementField, sign_statement_fields, statement_fields_from_v01,
			statement_proof_to_v01, unsigned_statement_signing_payload,
		},
	},
	platform::{
		CoreStorageKey, HostChatContact, HostChatPushRoute, HostPushDelivery, HostPushService,
		PushPlatform, normalize_chat_identifier,
	},
	runtime::{
		ProductRuntimeHost,
		authority::{AuthorityError, ChatDelivery},
		hop_client::{HopCallError, HopClient},
		until_cancelled,
	},
	unix_time::current_unix_millis,
};

/// The longest device token the relay takes.
const DEVICE_TOKEN_MAX_BYTES: usize = 512;

/// The most push routes a device keeps: one per contact.
const PUSH_ROUTES_MAX: usize = 4096;

/// The longest title of a chat notification, in Unicode scalar values.
const PUSH_TITLE_MAX_CHARS: usize = 64;

/// The most contacts the host keeps for the rest of the app.
const CONTACTS_MAX: usize = 4096;

/// The longest username of a contact the host keeps, in Unicode scalar values.
const CONTACT_USERNAME_MAX_CHARS: usize = 64;

/// The most bytes of the staged native chat history one `historyExport` gives.
const HISTORY_CHUNK_MAX: u32 = 1 << 20;

/// The largest encrypted notification: the native payload is at most 1,800
/// bytes before its encryption and framing.
const NOTIFICATION_MAX_BYTES: usize = 4096;

/// The longest chat request id: a UUID, with room to spare.
const REQUEST_ID_MAX_CHARS: usize = 64;

/// A chat request id names a derivation junction, so it is short and plain.
fn check_request_id(request_id: &str) -> PersonChatResult<()> {
	let plain = request_id.chars().all(|char| char.is_ascii_alphanumeric() || char == '-');
	if request_id.is_empty() || request_id.len() > REQUEST_ID_MAX_CHARS || !plain {
		return Err(invalid("a chat request id is 1 to 64 letters, digits or dashes"));
	}
	Ok(())
}

/// The largest file `hopUpload` takes and `hopDownload` gives, well inside
/// one 8 MiB TrUAPI message: these calls carry compaction batches (at most
/// 1,999,936 bytes), while media goes to the host's own renderer.
const HOP_FILE_MAX_BYTES: usize = 4 * 1024 * 1024;

type PersonChatResult<T> = Result<T, CallError<latest::HostPersonChatError>>;

fn token_bytes(service: HostPushService, token: &str) -> PersonChatResult<Vec<u8>> {
	match service {
		HostPushService::Apns => {
			hex::decode(token).map_err(|_| unknown("the host gave an APNs token that is not hex"))
		},
		HostPushService::Fcm => Ok(token.as_bytes().to_vec()),
	}
}

fn invalid(reason: impl Into<String>) -> CallError<latest::HostPersonChatError> {
	CallError::Domain(latest::HostPersonChatError::InvalidRequest { reason: reason.into() })
}

fn unknown(reason: impl Into<String>) -> CallError<latest::HostPersonChatError> {
	CallError::Domain(latest::HostPersonChatError::Unknown { reason: reason.into() })
}

fn authority_error(error: AuthorityError) -> CallError<latest::HostPersonChatError> {
	match error {
		AuthorityError::Disconnected => {
			CallError::Domain(latest::HostPersonChatError::NotConnected)
		},
		AuthorityError::Unavailable { .. } | AuthorityError::NotSupported { .. } => {
			CallError::Unsupported
		},
		error => unknown(error.to_string()),
	}
}

/// Refuses anything but a SCALE chat request message: a message id, a
/// timestamp and a known content version, whose V2 content names this
/// identity. Keeps the identity signature from serving as a generic oracle.
fn check_chat_request_message(message: &[u8], identity: &[u8; 32]) -> PersonChatResult<()> {
	let mut input = message;
	String::decode(&mut input).map_err(|_| invalid("the message has no message id"))?;
	u64::decode(&mut input).map_err(|_| invalid("the message has no timestamp"))?;
	match u8::decode(&mut input).map_err(|_| invalid("the message has no content"))? {
		0 => Ok(()),
		1 => {
			let named = <[u8; 32]>::decode(&mut input)
				.map_err(|_| invalid("the request names no identity"))?;
			if &named == identity {
				Ok(())
			} else {
				Err(invalid("the request names another identity"))
			}
		},
		version => Err(invalid(format!("unknown request content version {version}"))),
	}
}

fn identity_account(session: &SessionInfo) -> PersonChatResult<[u8; 32]> {
	session
		.identity_account_id
		.or_else(|| session.sso.as_ref().map(|sso| sso.identity_account_id))
		.ok_or_else(|| unknown("the session has no identity account"))
}

fn chat_identity_secret(session: &SessionInfo) -> PersonChatResult<[u8; 32]> {
	session
		.identity_chat_private_key
		.ok_or_else(|| unknown("the session has no chat identity key"))
}

fn hop_error(error: HopCallError) -> CallError<latest::HostPersonChatError> {
	unknown(format!("HOP node: {error}"))
}

impl ProductRuntimeHost {
	/// The signed-in session, for the chat product only. Any other product
	/// gets `Unsupported` before anything else runs.
	fn person_chat_session(&self) -> PersonChatResult<SessionInfo> {
		if !self.connection.product.is_person_chat_product() {
			return Err(CallError::Unsupported);
		}
		self.connection
			.session_state
			.current()
			.ok_or(CallError::Domain(latest::HostPersonChatError::NotConnected))
	}

	/// The key this device decrypts chat with. A linked device is announced by
	/// its wallet with the encryption key of its SSO session, as the native
	/// apps do, so it uses that key; the wallet device uses its device key.
	async fn chat_device_secret(&self, session: &SessionInfo) -> PersonChatResult<[u8; 32]> {
		match &session.sso {
			Some(sso) => Ok(sso.enc_secret),
			None => self.connection.services.chat_device_secret().await.map_err(unknown),
		}
	}

	/// The devices paired with this wallet device; none when the host does not say.
	async fn paired_devices(&self, cx: &CallContext) -> Vec<latest::PersonChatDevice> {
		let Some(platform) = self.connection.services.person_chat_platform() else {
			return Vec::new();
		};
		match until_cancelled(cx, platform.paired_devices()).await {
			Ok(Ok(devices)) => devices
				.into_iter()
				.map(|device| latest::PersonChatDevice {
					statement_account_id: device.statement_account_id,
					enc_public_key: device.encryption_public_key,
					name: device.name,
				})
				.collect(),
			Ok(Err(error)) => {
				tracing::warn!(reason = %error.reason, "paired devices could not be read");
				Vec::new()
			},
			Err(_) => Vec::new(),
		}
	}

	fn push_platform(&self) -> PersonChatResult<std::sync::Arc<dyn PushPlatform>> {
		self.connection.services.push_platform().ok_or(CallError::Unsupported)
	}

	async fn hop_nodes(&self, cx: &CallContext) -> PersonChatResult<Vec<String>> {
		let platform =
			self.connection.services.person_chat_platform().ok_or(CallError::Unsupported)?;
		let nodes = until_cancelled(cx, platform.hop_nodes())
			.await
			.map_err(|error| unknown(error.to_string()))?
			.map_err(|error| unknown(error.reason))?;
		Ok(nodes.into_iter().filter(|node| node.starts_with("wss://")).collect())
	}

	/// A client of `node`, which must be one of the host's HOP nodes.
	async fn hop_client(&self, cx: &CallContext, node: &str) -> PersonChatResult<HopClient> {
		if !self.hop_nodes(cx).await?.iter().any(|trusted| trusted == node) {
			return Err(invalid(format!("{node} is not a HOP node this host trusts")));
		}
		let connector = self.connection.services.hop_connector().ok_or(CallError::Unsupported)?;
		let transport = until_cancelled(cx, connector.connect(node))
			.await
			.map_err(|error| unknown(error.to_string()))?
			.map_err(hop_error)?;
		Ok(HopClient::new(transport))
	}

	/// Upload `data` as one file to a node the host picks at random, under a
	/// new ticket, signed by the product's Bulletin allowance account.
	/// `progress` follows the upload, as [`HopClient::upload`] reports it.
	async fn upload_hop_file_checked(
		&self,
		cx: &CallContext,
		data: Vec<u8>,
		progress: impl FnMut(u64, u64) + Send,
	) -> PersonChatResult<latest::HostPersonChatHopUploadResponse> {
		let nodes = self.hop_nodes(cx).await?;
		let mut pick = [0u8; 8];
		getrandom::getrandom(&mut pick).map_err(|error| unknown(error.to_string()))?;
		let node = nodes
			.get(u64::from_le_bytes(pick) as usize % nodes.len().max(1))
			.cloned()
			.ok_or_else(|| unknown("the host knows no HOP node"))?;
		let mut ticket = [0u8; 32];
		getrandom::getrandom(&mut ticket).map_err(|error| unknown(error.to_string()))?;
		let session = self
			.accounts
			.current_session()
			.ok_or(CallError::Domain(latest::HostPersonChatError::NotConnected))?;
		let allowance = self
			.hop_submit_allowance(cx, &session, data.len() as u64)
			.await
			.map_err(|reason| unknown(format!("Bulletin allowance: {reason}")))?;
		let signer = allowance_signer(&allowance).map_err(unknown)?;
		let client = self.hop_client(cx, &node).await?;
		let now_ms = current_unix_millis();
		let identifier =
			until_cancelled(cx, client.upload(&ticket, &data, &signer, now_ms, progress))
				.await
				.map_err(|error| unknown(error.to_string()))?
				.map_err(hop_error)?;
		if let Err(error) = self
			.connection
			.services
			.platform
			.write_core_storage(CoreStorageKey::HopUpload { identifier }, Vec::new())
			.await
		{
			tracing::warn!(reason = %error.reason, "the core could not remember a HOP upload");
		}
		Ok(latest::HostPersonChatHopUploadResponse { identifier, ticket, node })
	}

	/// [`Self::upload_hop_file_checked`] for the host's attachment path, with
	/// its failure as text.
	pub async fn upload_hop_file(
		&self,
		cx: &CallContext,
		data: Vec<u8>,
		progress: impl FnMut(u64, u64) + Send,
	) -> Result<latest::HostPersonChatHopUploadResponse, String> {
		self.upload_hop_file_checked(cx, data, progress)
			.await
			.map_err(|error| format!("{error:?}"))
	}

	async fn sign_as_identity(&self, payload: Vec<u8>) -> PersonChatResult<[u8; 64]> {
		let session = self
			.accounts
			.current_session()
			.ok_or(CallError::Domain(latest::HostPersonChatError::NotConnected))?;
		self.accounts.sign_as_identity(&session, payload).await.map_err(authority_error)
	}
}

/// The identifier, ticket and node of a [`latest::ChatResource::Hop`].
fn hop_resource(resource: &latest::ChatResource) -> Result<([u8; 32], [u8; 32], String), String> {
	let latest::ChatResource::Hop { identifier, ticket, node, .. } = resource;
	let identifier =
		identifier.as_slice().try_into().map_err(|_| "the identifier is not 32 bytes")?;
	let ticket = ticket.as_slice().try_into().map_err(|_| "the ticket is not 32 bytes")?;
	Ok((identifier, ticket, node.clone()))
}

impl ProductRuntimeHost {
	/// The bytes of a HOP file that a room of the chat product shows, and the
	/// pool entries to ack once the host keeps them. Claims only from a node
	/// the host lists, and falls back to Bulletin for promoted entries.
	/// `progress` follows the download as [`HopClient::download`] reports it.
	pub async fn open_hop_chat_resource(
		&self,
		resource: &latest::ChatResource,
		progress: impl FnMut(u64, u64) + Send,
	) -> Result<(Vec<u8>, Vec<[u8; 32]>), String> {
		if !self.connection.product.is_person_chat_product() {
			return Err("only the chat product's rooms carry HOP files".to_string());
		}
		let (identifier, ticket, node) = hop_resource(resource)?;
		let cx = CallContext::default();
		let client = self.hop_client(&cx, &node).await.map_err(|error| format!("{error:?}"))?;
		let (data, entries) = client
			.download(&identifier, &ticket, progress)
			.await
			.map_err(|error| error.to_string())?;
		if data.len() as u64 > crate::platform::CHAT_HOP_FILE_MAX_BYTES {
			return Err("the file is larger than a chat file may be".to_string());
		}
		Ok((data, entries))
	}

	/// Ack the pool entries [`Self::open_hop_chat_resource`] gave, once the
	/// host keeps the bytes, so the node drops them.
	pub async fn ack_hop_chat_resource(
		&self,
		resource: &latest::ChatResource,
		entries: &[[u8; 32]],
	) -> Result<(), String> {
		if !self.connection.product.is_person_chat_product() {
			return Err("only the chat product's rooms carry HOP files".to_string());
		}
		let (identifier, ticket, node) = hop_resource(resource)?;
		let uploaded_here = self
			.connection
			.services
			.platform
			.read_core_storage(CoreStorageKey::HopUpload { identifier })
			.await
			.map_err(|error| error.reason)?;
		if uploaded_here.is_some() {
			return Ok(());
		}
		let cx = CallContext::default();
		let client = self.hop_client(&cx, &node).await.map_err(|error| format!("{error:?}"))?;
		let keys = crate::host_logic::hop::ticket_keys(&ticket);
		for entry in entries {
			client.ack(entry, &keys).await.map_err(|error| error.to_string())?;
		}
		Ok(())
	}
}

#[truapi::async_trait]
impl PersonChat for ProductRuntimeHost {
	#[instrument(skip_all, fields(runtime.method = "person_chat.get_profile"))]
	async fn get_profile(
		&self,
		cx: &CallContext,
		request: HostPersonChatGetProfileRequest,
	) -> Result<HostPersonChatGetProfileResponse, CallError<HostPersonChatError>> {
		let version = request.version();
		let latest::HostPersonChatGetProfileRequest {} = request.into_latest();
		let result = async {
			let session = self.person_chat_session()?;
			let identity_account_id = identity_account(&session)?;
			let (role, statement_account_id) = match &session.sso {
				Some(sso) => (latest::PersonChatRole::Linked, sso.ss_public_key),
				None => (latest::PersonChatRole::Primary, identity_account_id),
			};
			let device_enc_public_key = x25519_public_key(self.chat_device_secret(&session).await?);
			// Peers address every device of the identity, this one included, so
			// the chat product announces it with the others.
			let this_device = latest::PersonChatDevice {
				statement_account_id,
				enc_public_key: device_enc_public_key,
				name: self.connection.services.host_info.name.clone(),
			};
			let mut own_devices = vec![this_device];
			if session.sso.is_none() {
				own_devices.extend(self.paired_devices(cx).await);
			}
			Ok(latest::HostPersonChatGetProfileResponse {
				identity_account_id,
				chat_public_key: x25519_public_key(chat_identity_secret(&session)?),
				lite_username: session.lite_username.clone(),
				full_username: session.full_username.clone(),
				role,
				statement_account_id,
				device_enc_public_key,
				own_devices,
			})
		}
		.await;
		result
			.map(|response| HostPersonChatGetProfileResponse::from_latest(response, version))
			.map_err(|error| from_latest_error(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "person_chat.chat_secrets"))]
	async fn chat_secrets(
		&self,
		_cx: &CallContext,
		request: HostPersonChatSecretsRequest,
	) -> Result<HostPersonChatSecretsResponse, CallError<HostPersonChatError>> {
		let version = request.version();
		let latest::HostPersonChatSecretsRequest {} = request.into_latest();
		let result = async {
			let session = self.person_chat_session()?;
			Ok(latest::HostPersonChatSecretsResponse {
				chat_identity_secret: chat_identity_secret(&session)?,
				device_enc_secret: self.chat_device_secret(&session).await?,
			})
		}
		.await;
		result
			.map(|response| HostPersonChatSecretsResponse::from_latest(response, version))
			.map_err(|error| from_latest_error(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "person_chat.prove_statement"))]
	async fn prove_statement(
		&self,
		_cx: &CallContext,
		request: HostPersonChatProveStatementRequest,
	) -> Result<HostPersonChatProveStatementResponse, CallError<HostPersonChatError>> {
		let version = request.version();
		let latest::HostPersonChatProveStatementRequest { statement } = request.into_latest();
		let result = async {
			let session = self.person_chat_session()?;
			let fields = statement_fields_from_v01(statement).map_err(invalid)?;
			let proof = match &session.sso {
				Some(sso) => sign_statement_fields(sso.ss_secret, sso.ss_public_key, fields)
					.map_err(unknown)?
					.into_iter()
					.find_map(|field| match field {
						StatementField::Proof(proof) => Some(statement_proof_to_v01(proof)),
						_ => None,
					})
					.ok_or_else(|| unknown("the signed statement has no proof"))?,
				None => {
					let signer = identity_account(&session)?;
					let payload = unsigned_statement_signing_payload(fields).map_err(invalid)?;
					let signature = self.sign_as_identity(payload).await?;
					latest::StatementProof::Sr25519 { signature, signer }
				},
			};
			Ok(latest::HostPersonChatProveStatementResponse { proof })
		}
		.await;
		result
			.map(|response| HostPersonChatProveStatementResponse::from_latest(response, version))
			.map_err(|error| from_latest_error(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "person_chat.prove_chat_request"))]
	async fn prove_chat_request(
		&self,
		_cx: &CallContext,
		request: HostPersonChatProveChatRequestRequest,
	) -> Result<HostPersonChatProveChatRequestResponse, CallError<HostPersonChatError>> {
		let version = request.version();
		let latest::HostPersonChatProveChatRequestRequest { message, acceptor } =
			request.into_latest();
		let result = async {
			let session = self.person_chat_session()?;
			check_chat_request_message(&message, &identity_account(&session)?)?;
			let mut payload = message;
			Compact(acceptor.len() as u32).encode_to(&mut payload);
			payload.extend_from_slice(&acceptor);
			let signature = self.sign_as_identity(payload).await?;
			Ok(latest::HostPersonChatProveChatRequestResponse { signature })
		}
		.await;
		result
			.map(|response| HostPersonChatProveChatRequestResponse::from_latest(response, version))
			.map_err(|error| from_latest_error(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "person_chat.push_tokens"))]
	async fn push_tokens(
		&self,
		cx: &CallContext,
		request: HostPersonChatPushTokensRequest,
	) -> Result<HostPersonChatPushTokensResponse, CallError<HostPersonChatError>> {
		let version = request.version();
		let latest::HostPersonChatPushTokensRequest {} = request.into_latest();
		let result = async {
			self.person_chat_session()?;
			let platform = self.push_platform()?;
			let tokens = until_cancelled(cx, platform.push_tokens())
				.await
				.map_err(|error| unknown(error.to_string()))?
				.map_err(|error| unknown(error.reason))?;
			let push_platform = match tokens.service {
				HostPushService::Apns => latest::PersonChatPushPlatform::Ios,
				HostPushService::Fcm => latest::PersonChatPushPlatform::Android,
			};
			let push = match tokens.token.as_deref() {
				Some(token) => Some(latest::PersonChatPushToken {
					token: token_bytes(tokens.service, token)?,
					platform: push_platform,
				}),
				None => None,
			};
			let voip = match tokens.voip_token.as_deref() {
				Some(token) => Some(token_bytes(HostPushService::Apns, token)?),
				None => None,
			};
			Ok(latest::HostPersonChatPushTokensResponse { push, voip })
		}
		.await;
		result
			.map(|response| HostPersonChatPushTokensResponse::from_latest(response, version))
			.map_err(|error| from_latest_error(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "person_chat.notify_peer"))]
	async fn notify_peer(
		&self,
		cx: &CallContext,
		request: HostPersonChatNotifyPeerRequest,
	) -> Result<HostPersonChatNotifyPeerResponse, CallError<HostPersonChatError>> {
		let version = request.version();
		let latest::HostPersonChatNotifyPeerRequest {
			device_token,
			push_id,
			platform,
			message,
			voip,
		} = request.into_latest();
		let result = async {
			self.person_chat_session()?;
			if device_token.is_empty() || device_token.len() > DEVICE_TOKEN_MAX_BYTES {
				return Err(invalid("the device token is empty or too long"));
			}
			if message.is_empty() || message.len() > NOTIFICATION_MAX_BYTES {
				return Err(invalid("the notification is empty or too large"));
			}
			let push_platform = self.push_platform()?;
			let delivery = HostPushDelivery {
				service: match platform {
					latest::PersonChatPushPlatform::Ios => HostPushService::Apns,
					latest::PersonChatPushPlatform::Android => HostPushService::Fcm,
				},
				token: device_token,
				push_id: hex::encode(push_id),
				message: hex::encode(message),
				voip,
			};
			let delivered =
				until_cancelled(cx, push_platform.send_push(&self.connection.product, delivery))
					.await
					.map_err(|error| unknown(error.to_string()))?
					.map_err(|error| unknown(error.reason))?;
			Ok(latest::HostPersonChatNotifyPeerResponse { delivered })
		}
		.await;
		result
			.map(|response| HostPersonChatNotifyPeerResponse::from_latest(response, version))
			.map_err(|error| from_latest_error(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "person_chat.set_push_routes"))]
	async fn set_push_routes(
		&self,
		cx: &CallContext,
		request: HostPersonChatSetPushRoutesRequest,
	) -> Result<HostPersonChatSetPushRoutesResponse, CallError<HostPersonChatError>> {
		let version = request.version();
		let latest::HostPersonChatSetPushRoutesRequest { routes } = request.into_latest();
		let result = async {
			self.person_chat_session()?;
			if routes.len() > PUSH_ROUTES_MAX {
				return Err(invalid(format!("more than {PUSH_ROUTES_MAX} push routes")));
			}
			let routes = routes
				.into_iter()
				.map(|route| {
					let room_id = normalize_chat_identifier("room_id", &route.room_id)
						.map_err(|_| invalid(format!("{} is not a room id", route.room_id)))?;
					Ok(HostChatPushRoute {
						push_id: hex::encode(route.push_id),
						room_id,
						title: route.title.chars().take(PUSH_TITLE_MAX_CHARS).collect(),
						key: route.key.to_vec(),
					})
				})
				.collect::<PersonChatResult<Vec<_>>>()?;
			let platform = self.push_platform()?;
			until_cancelled(cx, platform.set_chat_push_routes(&self.connection.product, routes))
				.await
				.map_err(|error| unknown(error.to_string()))?
				.map_err(|error| unknown(error.reason))
		}
		.await;
		result
			.map(|response| HostPersonChatSetPushRoutesResponse::from_latest(response, version))
			.map_err(|error| from_latest_error(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "person_chat.publish_contacts"))]
	async fn publish_contacts(
		&self,
		cx: &CallContext,
		request: HostPersonChatPublishContactsRequest,
	) -> Result<HostPersonChatPublishContactsResponse, CallError<HostPersonChatError>> {
		let version = request.version();
		let latest::HostPersonChatPublishContactsRequest { contacts } = request.into_latest();
		let result = async {
			self.person_chat_session()?;
			if contacts.len() > CONTACTS_MAX {
				return Err(invalid(format!("more than {CONTACTS_MAX} contacts")));
			}
			let contacts = contacts
				.into_iter()
				.map(|contact| {
					let room_id = normalize_chat_identifier("room_id", &contact.room_id)
						.map_err(|_| invalid(format!("{} is not a room id", contact.room_id)))?;
					Ok(HostChatContact {
						account_id: contact.account_id,
						username: contact
							.username
							.map(|name| name.chars().take(CONTACT_USERNAME_MAX_CHARS).collect()),
						room_id,
						blocked: contact.blocked,
					})
				})
				.collect::<PersonChatResult<Vec<_>>>()?;
			let platform =
				self.connection.services.person_chat_platform().ok_or(CallError::Unsupported)?;
			until_cancelled(cx, platform.set_contacts(&self.connection.product, contacts))
				.await
				.map_err(|error| unknown(error.to_string()))?
				.map_err(|error| unknown(error.reason))
		}
		.await;
		result
			.map(|response| HostPersonChatPublishContactsResponse::from_latest(response, version))
			.map_err(|error| from_latest_error(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "person_chat.history_export"))]
	async fn history_export(
		&self,
		cx: &CallContext,
		request: HostPersonChatHistoryExportRequest,
	) -> Result<HostPersonChatHistoryExportResponse, CallError<HostPersonChatError>> {
		let version = request.version();
		let latest::HostPersonChatHistoryExportRequest { offset } = request.into_latest();
		let result = async {
			self.person_chat_session()?;
			let platform =
				self.connection.services.person_chat_platform().ok_or(CallError::Unsupported)?;
			let chunk = until_cancelled(
				cx,
				platform.history_export(&self.connection.product, offset, HISTORY_CHUNK_MAX),
			)
			.await
			.map_err(|error| unknown(error.to_string()))?
			.map_err(|error| unknown(error.reason))?;
			let end = offset.checked_add(chunk.data.len() as u64);
			if chunk.data.len() > HISTORY_CHUNK_MAX as usize ||
				end.is_none_or(|end| end > chunk.total)
			{
				return Err(unknown(format!(
					"the host gave {} bytes at {offset} of a {}-byte history",
					chunk.data.len(),
					chunk.total
				)));
			}
			Ok(latest::HostPersonChatHistoryExportResponse { total: chunk.total, data: chunk.data })
		}
		.await;
		result
			.map(|response| HostPersonChatHistoryExportResponse::from_latest(response, version))
			.map_err(|error| from_latest_error(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "person_chat.history_imported"))]
	async fn history_imported(
		&self,
		cx: &CallContext,
		request: HostPersonChatHistoryImportedRequest,
	) -> Result<HostPersonChatHistoryImportedResponse, CallError<HostPersonChatError>> {
		let version = request.version();
		let latest::HostPersonChatHistoryImportedRequest {} = request.into_latest();
		let result = async {
			self.person_chat_session()?;
			let platform =
				self.connection.services.person_chat_platform().ok_or(CallError::Unsupported)?;
			until_cancelled(cx, platform.history_imported(&self.connection.product))
				.await
				.map_err(|error| unknown(error.to_string()))?
				.map_err(|error| unknown(error.reason))
		}
		.await;
		result
			.map(|response| HostPersonChatHistoryImportedResponse::from_latest(response, version))
			.map_err(|error| from_latest_error(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "person_chat.hop_upload"))]
	async fn hop_upload(
		&self,
		cx: &CallContext,
		request: HostPersonChatHopUploadRequest,
	) -> Result<HostPersonChatHopUploadResponse, CallError<HostPersonChatError>> {
		let version = request.version();
		let latest::HostPersonChatHopUploadRequest { data } = request.into_latest();
		let result = async {
			self.person_chat_session()?;
			if data.is_empty() || data.len() > HOP_FILE_MAX_BYTES {
				return Err(invalid(format!("a file must be 1 to {HOP_FILE_MAX_BYTES} bytes")));
			}
			self.upload_hop_file_checked(cx, data, |_, _| {}).await
		}
		.await;
		result
			.map(|response| HostPersonChatHopUploadResponse::from_latest(response, version))
			.map_err(|error| from_latest_error(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "person_chat.hop_download"))]
	async fn hop_download(
		&self,
		cx: &CallContext,
		request: HostPersonChatHopDownloadRequest,
	) -> Result<HostPersonChatHopDownloadResponse, CallError<HostPersonChatError>> {
		let version = request.version();
		let latest::HostPersonChatHopDownloadRequest { identifier, ticket, node } =
			request.into_latest();
		let result = async {
			self.person_chat_session()?;
			let client = self.hop_client(cx, &node).await?;
			let (data, entries) =
				until_cancelled(cx, client.download(&identifier, &ticket, |_, _| {}))
					.await
					.map_err(|error| unknown(error.to_string()))?
					.map_err(hop_error)?;
			if data.len() > HOP_FILE_MAX_BYTES {
				return Err(unknown(format!("the file is larger than {HOP_FILE_MAX_BYTES} bytes")));
			}
			Ok(latest::HostPersonChatHopDownloadResponse { data, entries })
		}
		.await;
		result
			.map(|response| HostPersonChatHopDownloadResponse::from_latest(response, version))
			.map_err(|error| from_latest_error(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "person_chat.hop_ack"))]
	async fn hop_ack(
		&self,
		cx: &CallContext,
		request: HostPersonChatHopAckRequest,
	) -> Result<HostPersonChatHopAckResponse, CallError<HostPersonChatError>> {
		let version = request.version();
		let latest::HostPersonChatHopAckRequest { node, ticket, entries } = request.into_latest();
		let result = async {
			self.person_chat_session()?;
			let client = self.hop_client(cx, &node).await?;
			let keys = crate::host_logic::hop::ticket_keys(&ticket);
			for entry in &entries {
				until_cancelled(cx, client.ack(entry, &keys))
					.await
					.map_err(|error| unknown(error.to_string()))?
					.map_err(hop_error)?;
			}
			Ok(())
		}
		.await;
		result
			.map(|response| HostPersonChatHopAckResponse::from_latest(response, version))
			.map_err(|error| from_latest_error(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "person_chat.prepare_delivery"))]
	async fn prepare_delivery(
		&self,
		_cx: &CallContext,
		request: HostPersonChatPrepareDeliveryRequest,
	) -> Result<HostPersonChatPrepareDeliveryResponse, CallError<HostPersonChatError>> {
		let version = request.version();
		let latest::HostPersonChatPrepareDeliveryRequest { request_id, size } =
			request.into_latest();
		let result = async {
			self.person_chat_session()?;
			check_request_id(&request_id)?;
			let session = self
				.accounts
				.current_session()
				.ok_or(CallError::Domain(latest::HostPersonChatError::NotConnected))?;
			let delivery = match self
				.accounts
				.prepare_chat_delivery(&session, request_id, size)
				.await
				.map_err(authority_error)?
			{
				ChatDelivery::Anonymous { period, account_id } => {
					latest::PersonChatDelivery::Anonymous { period, account_id }
				},
				ChatDelivery::NoSlot => latest::PersonChatDelivery::NoSlot,
			};
			Ok(latest::HostPersonChatPrepareDeliveryResponse { delivery })
		}
		.await;
		result
			.map(|response| HostPersonChatPrepareDeliveryResponse::from_latest(response, version))
			.map_err(|error| from_latest_error(error, version))
	}

	#[instrument(skip_all, fields(runtime.method = "person_chat.prove_delivery"))]
	async fn prove_delivery(
		&self,
		_cx: &CallContext,
		request: HostPersonChatProveDeliveryRequest,
	) -> Result<HostPersonChatProveDeliveryResponse, CallError<HostPersonChatError>> {
		let version = request.version();
		let latest::HostPersonChatProveDeliveryRequest { request_id, period, statement } =
			request.into_latest();
		let result = async {
			self.person_chat_session()?;
			check_request_id(&request_id)?;
			let fields = statement_fields_from_v01(statement).map_err(invalid)?;
			let payload = unsigned_statement_signing_payload(fields).map_err(invalid)?;
			let session = self
				.accounts
				.current_session()
				.ok_or(CallError::Domain(latest::HostPersonChatError::NotConnected))?;
			let (signer, signature) = self
				.accounts
				.sign_as_chat_delivery(&session, request_id, period, payload)
				.await
				.map_err(authority_error)?;
			Ok(latest::HostPersonChatProveDeliveryResponse {
				proof: latest::StatementProof::Sr25519 { signature, signer },
			})
		}
		.await;
		result
			.map(|response| HostPersonChatProveDeliveryResponse::from_latest(response, version))
			.map_err(|error| from_latest_error(error, version))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn message(content: &[u8]) -> Vec<u8> {
		let mut message = "6f1d".to_string().encode();
		1_763_164_800_000u64.encode_to(&mut message);
		message.extend_from_slice(content);
		message
	}

	#[test]
	fn chat_request_messages_are_the_only_shape_signed() {
		let identity = [7u8; 32];
		let mut v2 = vec![1u8];
		v2.extend_from_slice(&identity);
		let mut other_identity = vec![1u8];
		other_identity.extend_from_slice(&[8u8; 32]);

		assert_eq!(check_chat_request_message(&message(&[0]), &identity), Ok(()));
		assert_eq!(check_chat_request_message(&message(&v2), &identity), Ok(()));
		// A request naming someone else would let the product speak for them.
		assert_eq!(
			check_chat_request_message(&message(&other_identity), &identity),
			Err(invalid("the request names another identity"))
		);
		assert_eq!(
			check_chat_request_message(&message(&[2]), &identity),
			Err(invalid("unknown request content version 2"))
		);
		// Bytes that are no chat request at all, such as an extrinsic payload.
		assert!(check_chat_request_message(&[0xff, 0xff, 0xff], &identity).is_err());
	}
}
