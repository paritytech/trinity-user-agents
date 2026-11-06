//! Signing-host responder half of the host-spec §B pairing protocol.
//!
//! Answers a pairing host's handshake proposal (QR/deeplink) with an
//! encrypted `Success` statement, then serves the encrypted SSO session:
//! acks every inbound request statement, dispatches the batched
//! [`v1::RemoteMessage`] requests onto the local signing authority, and posts
//! the response statements the pairing host is waiting for. Runs until the
//! peer sends `Disconnected`, the local session ends, or the transport fails.
//!
//! Sensitive operations consult [`crate::platform::UserConfirmation`], the
//! same seam browser hosts use for their confirmation modals; a headless host
//! implements it with its approval policy.

mod replay;

use std::{
	collections::{HashMap, HashSet, VecDeque, hash_map::Entry},
	sync::{Arc, Mutex},
	time::Instant,
};

use core::{future::Future, ops::ControlFlow, time::Duration};

use futures::{
	FutureExt, Stream, StreamExt,
	future::{Fuse, FusedFuture},
	pin_mut,
};
use parity_scale_codec::Encode;
use tracing::{debug, instrument, warn};

use crate::{
	host_internal::{
		sso_messages::{
			IncomingSsoRequest, RemoteMessage, RemoteMessageData, SsoResponseCode,
			build_outgoing_request_statement, build_signed_session_response_statement,
			decode_incoming_sso_request, v1,
		},
		sso_wire::ResponseOutcome,
	},
	host_logic::{
		entropy::root_entropy_source,
		product_account::{
			ProductAccountError, derive_identity_keypair, derive_root_keypair_from_entropy,
		},
		session::SsoSessionInfo,
		sso::pairing::{
			PairingDeeplinkDecodeError, ResponderIdentity, VersionedHandshakeProposal,
			bootstrap_topic, decode_pairing_deeplink, derive_identity_chat_private_key,
			derive_x25519_keypair_from_entropy, encrypt_v2_handshake_response,
			establish_responder_session_info, v2,
		},
		statement_store::{build_signed_statement, parse_new_statements_result},
		x25519_seal::X25519PrivateKey,
	},
	runtime::{
		services::RuntimeServices,
		signing_host::{SigningHost, SigningHostSsoService},
		sso_remote::{fresh_statement_expiry, sso_message_id},
		sso_service::{Dispatch, SsoWithdrawals},
		statement_store_rpc::{self, StatementSubmitError},
	},
	time::{Backoff, retry},
	unix_time::current_unix_secs as statement_current_unix_secs,
};
use replay::{ReplayExecution, ReplayRequest, SsoReplayLocks, SsoReplayScope, execute_once};

/// RFC-0022 domain for the responder's persistent SSO X25519 key.
const SSO_ENCRYPTION_DOMAIN: &[u8] = b"sso";

/// Upper bound on undecodable request ids acknowledged within one serve loop.
const MAX_DECODE_FAILURE_REQUEST_IDS: usize = 1024;

/// Resends under [`Resend::Briefly`], after 1, 2, 4 and 8 seconds: about 15
/// seconds in all.
const MAX_RESENDS: u32 = 4;

/// Pauses between resends, growing to at most 8 seconds.
#[cfg(not(test))]
const RESEND_PAUSES: Backoff = Backoff::new(Duration::from_secs(1), Duration::from_secs(8));
#[cfg(test)]
const RESEND_PAUSES: Backoff = Backoff::new(Duration::from_millis(1), Duration::from_millis(8));

/// How long to resend a statement while the People chain is unreachable.
#[derive(Clone, Copy)]
enum Resend {
	/// Up to [`MAX_RESENDS`] times, when the caller can recover from losing it.
	Briefly,
	/// Until the store answers, for the answer to a request that already ran
	/// and will not run again.
	UntilAnswered,
}

impl Resend {
	/// Whether the statement may be sent again after `sends` sends.
	fn allows_another_after(self, sends: u32) -> bool {
		match self {
			Self::Briefly => sends <= MAX_RESENDS,
			Self::UntilAnswered => true,
		}
	}
}

fn derive_responder_identity(
	entropy: &[u8],
	network_suffix: &str,
) -> Result<(ResponderIdentity, [u8; 32]), ProductAccountError> {
	let statement = derive_identity_keypair(entropy, network_suffix)?;
	let (encryption_secret_key, encryption_public_key) =
		derive_x25519_keypair_from_entropy(entropy, SSO_ENCRYPTION_DOMAIN);
	let identity_chat_private_key = derive_identity_chat_private_key(entropy);
	Ok((
		ResponderIdentity {
			statement_secret: statement.secret.to_bytes(),
			statement_public_key: statement.public.to_bytes(),
			encryption_secret_key,
			encryption_public_key,
		},
		identity_chat_private_key,
	))
}

/// Bounded set of undecodable request ids acknowledged within one serve loop.
struct DecodeFailureRequestIds {
	seen: HashSet<String>,
	order: VecDeque<String>,
}

impl DecodeFailureRequestIds {
	fn new() -> Self {
		Self { seen: HashSet::new(), order: VecDeque::new() }
	}

	/// Record `request_id`, returning `true` if it was not already served.
	/// Evicts the oldest id when the capacity is exceeded.
	fn insert(&mut self, request_id: String) -> bool {
		if !self.seen.insert(request_id.clone()) {
			return false;
		}
		self.order.push_back(request_id);
		if self.order.len() > MAX_DECODE_FAILURE_REQUEST_IDS &&
			let Some(evicted) = self.order.pop_front()
		{
			self.seen.remove(&evicted);
		}
		true
	}
}

/// Terminal outcome of one responder serve loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum ResponderExit {
	/// The pairing host announced `Disconnected`; its durable pairing may be removed.
	PeerDisconnected,
	/// The statement subscription ended without a disconnect message; retain the pairing and retry.
	SubscriptionEnded,
}

/// Why [`crate::SigningHostRuntime::resume_pairing`] stopped serving.
///
/// Either way the peer is still paired.
#[derive(Debug, derive_more::Display, derive_more::Error)]
pub enum ResumePairingError {
	/// The session could not be served at all: this signing host has no active
	/// local session, or the session keys could not be derived. Serving again
	/// fails the same way until the local session changes.
	#[display("{reason}")]
	CannotServe {
		/// Human-readable failure reason.
		reason: String,
	},
	/// Serving failed after it started. Serving again may succeed.
	#[display("{reason}")]
	Interrupted {
		/// Human-readable failure reason.
		reason: String,
	},
}

/// Public key material identifying one pairing host's resumable SSO session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct PairedSsoPeer {
	/// Pairing host's statement-store account id.
	pub statement_account_id: [u8; 32],
	/// Pairing host's X25519 public key.
	pub encryption_public_key: [u8; 32],
}

/// Peer-supplied description of the host proposing a pairing.
///
/// Sanitized for direct rendering: every value is trimmed, stripped of control
/// characters and bidirectional overrides, capped at
/// [`MAX_PAIRING_METADATA_CHARS`] characters, and left `None` when nothing
/// survives. A value the peer did not send is `None` too.
///
/// Sanitized is not verified. Nothing signs this metadata, so a prompt built
/// from it states what the peer calls itself, never who it is.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct PairingProposalMetadata {
	/// Human-readable host name.
	pub host_name: Option<String>,
	/// Host software version.
	pub host_version: Option<String>,
	/// Host icon URL.
	pub host_icon: Option<String>,
	/// Platform kind, such as a browser or operating system name.
	pub platform_type: Option<String>,
	/// Platform version.
	pub platform_version: Option<String>,
}

/// Everything a pairing deeplink offers a host before it answers.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct PairingProposal {
	/// Key material this pairing would be established and resumed on.
	pub peer: PairedSsoPeer,
	/// How the peer describes itself on the pairing prompt.
	pub metadata: PairingProposalMetadata,
}

/// Host notified when a device finishes pairing with this signing host.
///
/// The core has no chat of its own, so announcing a new device to the user's
/// existing contacts belongs to the host.
///
/// Arrives on the thread answering the handshake, while the pairing call is
/// still running, so hand the device off rather than announcing it inline.
pub trait DevicePairingObserver: Send + Sync {
	/// `device` paired: its handshake answer is on the Statement Store.
	///
	/// At least once per pairing, so a device that pairs again is reported
	/// again with the same value. The answer reaching the store is not proof
	/// the peer read it: one that cancelled or timed out waiting leaves a
	/// device here that never connects.
	fn device_paired(&self, device: PairedSsoPeer);
}

struct EstablishedPairing {
	session: SsoSessionInfo,
	replay_scope: SsoReplayScope,
}

impl PairedSsoPeer {
	/// Extract the public peer material carried by a pairing deeplink.
	pub fn from_deeplink(deeplink: &str) -> Result<Self, PairingDeeplinkDecodeError> {
		Ok(PairingProposal::from_deeplink(deeplink)?.peer)
	}

	/// The replay ledger scope for this peer's requests to `root_public_key`.
	fn replay_scope(self, root_public_key: [u8; 32]) -> SsoReplayScope {
		SsoReplayScope {
			root_public_key,
			peer_statement_account_id: self.statement_account_id,
			peer_encryption_public_key: self.encryption_public_key,
		}
	}
}

/// Longest metadata value a pairing prompt renders; the rest is dropped.
pub const MAX_PAIRING_METADATA_CHARS: usize = 512;

impl PairingProposal {
	/// Decode a pairing deeplink into the peer it advertises and the metadata
	/// a host prompts with.
	pub fn from_deeplink(deeplink: &str) -> Result<Self, PairingDeeplinkDecodeError> {
		let VersionedHandshakeProposal::V2(proposal) = decode_pairing_deeplink(deeplink)?;
		let mut metadata = PairingProposalMetadata::default();
		for v2::MetadataEntry(key, value) in proposal.metadata {
			let value = sanitize_pairing_metadata(value);
			match key {
				v2::MetadataKey::HostName => metadata.host_name = value,
				v2::MetadataKey::HostVersion => metadata.host_version = value,
				v2::MetadataKey::HostIcon => metadata.host_icon = value,
				v2::MetadataKey::PlatformType => metadata.platform_type = value,
				v2::MetadataKey::PlatformVersion => metadata.platform_version = value,
				v2::MetadataKey::Custom(_) => {},
			}
		}
		Ok(Self {
			peer: PairedSsoPeer {
				statement_account_id: proposal.device.statement_account_id,
				encryption_public_key: proposal.device.encryption_public_key,
			},
			metadata,
		})
	}
}

/// Make one peer-supplied metadata value safe to render.
///
/// Control characters and the bidirectional overrides drop out, because a
/// prompt that concatenates this value with its own text is otherwise a
/// surface for reordering what the user reads. The cap bounds what an
/// unbounded field can push into that prompt.
fn sanitize_pairing_metadata(value: String) -> Option<String> {
	let value = value
		.trim()
		.chars()
		.filter(|character| {
			!character.is_control() &&
				!matches!(
					character,
					'\u{061c}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
				)
		})
		.take(MAX_PAIRING_METADATA_CHARS)
		.collect::<String>();
	(!value.is_empty()).then_some(value)
}

/// Incoming SSO for one signing host: pairing answers, served sessions and
/// duplicate detection.
pub struct SsoResponderService {
	services: Arc<RuntimeServices>,
	signing_host: Arc<SigningHost>,
	/// Serializes replay-ledger updates within each wallet and peer scope.
	replay_locks: SsoReplayLocks,
	unpublished_answers: UnpublishedAnswers,
}

/// Refused answers, published when their request is redelivered instead of
/// running it again.
///
/// Holds the newest request per peer, the only one its channel can redeliver.
/// In memory only.
#[derive(Default)]
struct UnpublishedAnswers {
	by_peer: Mutex<HashMap<SsoReplayScope, (String, Vec<RemoteMessage>)>>,
}

impl UnpublishedAnswers {
	/// Keep `answers` to `request_id`, replacing those kept for an older request.
	fn keep(&self, scope: SsoReplayScope, request_id: &str, answers: Vec<RemoteMessage>) {
		if answers.is_empty() {
			return;
		}
		self.by_peer
			.lock()
			.expect("unpublished SSO answers mutex poisoned")
			.insert(scope, (request_id.to_string(), answers));
	}

	/// Take the answers kept for `request_id`.
	fn take(&self, scope: SsoReplayScope, request_id: &str) -> Vec<RemoteMessage> {
		match self
			.by_peer
			.lock()
			.expect("unpublished SSO answers mutex poisoned")
			.entry(scope)
		{
			Entry::Occupied(kept) if kept.get().0 == request_id => kept.remove().1,
			_ => Vec::new(),
		}
	}
}

impl SsoResponderService {
	/// Answer peers of `signing_host` over the runtime's statement store.
	pub fn new(services: Arc<RuntimeServices>, signing_host: Arc<SigningHost>) -> Arc<Self> {
		Arc::new(Self {
			services,
			signing_host,
			replay_locks: SsoReplayLocks::default(),
			unpublished_answers: UnpublishedAnswers::default(),
		})
	}

	/// Answer `deeplink` and serve the resulting SSO session until it ends.
	#[instrument(skip_all, fields(runtime.method = "sso_responder.respond_to_pairing"))]
	pub async fn respond_to_pairing(&self, deeplink: &str) -> Result<ResponderExit, String> {
		let established = self.establish_pairing_session(deeplink).await?;
		serve_session(self, established.session, established.replay_scope).await
	}

	/// Answer a pairing host's handshake without entering its long-lived serve loop.
	pub async fn establish_pairing(&self, deeplink: &str) -> Result<(), String> {
		self.establish_pairing_session(deeplink).await?;
		Ok(())
	}

	async fn establish_pairing_session(
		&self,
		deeplink: &str,
	) -> Result<EstablishedPairing, String> {
		let peer = PairedSsoPeer::from_deeplink(deeplink).map_err(|err| err.to_string())?;
		let entropy = self
			.signing_host
			.root_entropy()
			.map_err(|err| format!("signing host has no active local session: {err}"))?;
		// Product accounts and the SSO statement identity derive from the
		// canonical root key; the identity is the RFC-0022 `uid.<suffix>` default
		// account of the network this host is configured for.
		let root = derive_root_keypair_from_entropy(&entropy)
			.map_err(|err| format!("root account derivation failed: {err}"))?;
		let (identity, identity_chat_private_key) =
			derive_responder_identity(&entropy, self.signing_host.network_suffix())
				.map_err(|err| format!("responder identity derivation failed: {err}"))?;
		let device_enc_secret =
			X25519PrivateKey::from(self.services.device_encryption_secret().await?);
		let device_enc_pub_key = device_enc_secret.public_key().0;
		let session = responder_session_from_identity(&identity, peer)?;

		let success = v2::EncryptedResponse::Success(Box::new(v2::Success {
			identity_account_id: identity.statement_public_key,
			root_account_id: root.public.to_bytes(),
			identity_chat_private_key,
			sso_enc_pub_key: identity.encryption_public_key,
			device_enc_pub_key,
			root_entropy_source: root_entropy_source(&entropy),
		}));
		submit_handshake_answer(
			&self.services,
			&session,
			peer,
			&success,
			"sso-responder handshake",
		)
		.await?;
		debug!("answered pairing handshake");
		// The submit is the earliest point the peer could read the answer.
		if let Some(observer) = self.services.device_pairing_observer() {
			observer.device_paired(peer);
		}

		Ok(EstablishedPairing { session, replay_scope: peer.replay_scope(root.public.to_bytes()) })
	}

	/// Resume a previously paired SSO session from its persisted public peer keys.
	pub async fn resume_pairing(
		&self,
		peer: PairedSsoPeer,
	) -> Result<ResponderExit, ResumePairingError> {
		let cannot_serve = |reason| ResumePairingError::CannotServe { reason };
		let entropy = self.signing_host.root_entropy().map_err(|err| {
			cannot_serve(format!("signing host has no active local session: {err}"))
		})?;
		let root = derive_root_keypair_from_entropy(&entropy)
			.map_err(|err| cannot_serve(format!("root account derivation failed: {err}")))?;
		let session = responder_session(&entropy, self.signing_host.network_suffix(), peer)
			.map_err(cannot_serve)?;
		serve_session(self, session, peer.replay_scope(root.public.to_bytes()))
			.await
			.map_err(|reason| ResumePairingError::Interrupted { reason })
	}

	/// Notify a paired host that this signing host is ending their SSO session.
	pub async fn disconnect_paired_host(&self, peer: PairedSsoPeer) -> Result<(), String> {
		let entropy = self
			.signing_host
			.root_entropy()
			.map_err(|err| format!("signing host has no active local session: {err}"))?;
		let session = responder_session(&entropy, self.signing_host.network_suffix(), peer)?;
		let message_id = sso_message_id();
		let message = RemoteMessage {
			message_id: message_id.clone(),
			data: RemoteMessageData::V1(v1::RemoteMessage::Disconnected),
		};
		let statement = build_outgoing_request_statement(
			&session,
			message_id,
			vec![message],
			fresh_statement_expiry(),
		)?;
		publish(&self.services, &statement, "sso-responder disconnect", Resend::Briefly).await
	}

	/// Tell the pairing host that allowance allocation is under way, moving it off
	/// its QR screen for however long the allocation takes.
	///
	/// Answering at all needs this host's own statement-store allowance, which is
	/// the `WalletSso` renewal target, so callers register that before calling.
	/// Callers own the matching [`Self::notify_pairing_failed`]: the host has
	/// dropped its QR and waits without a deadline, so an allocation that then
	/// fails leaves it waiting forever unless it is told.
	pub async fn notify_pairing_allowance_allocation(
		&self,
		deeplink: &str,
	) -> Result<AnnouncedPairing, String> {
		let peer = PairedSsoPeer::from_deeplink(deeplink).map_err(|err| err.to_string())?;
		let entropy = self
			.signing_host
			.root_entropy()
			.map_err(|err| format!("signing host has no active local session: {err}"))?;
		let (identity, _) = derive_responder_identity(&entropy, self.signing_host.network_suffix())
			.map_err(|err| format!("responder identity derivation failed: {err}"))?;
		let session = responder_session_from_identity(&identity, peer)?;

		let pending = v2::EncryptedResponse::Pending(v2::Status::AllowanceAllocation);
		submit_handshake_answer(
			&self.services,
			&session,
			peer,
			&pending,
			"sso-responder allowance allocation",
		)
		.await?;
		debug!("told pairing host that allowance allocation started");
		Ok(AnnouncedPairing { identity, peer })
	}

	/// Tell the pairing host that pairing failed, so it reports `reason` and offers
	/// a retry rather than waiting on an answer that is never coming.
	pub async fn notify_pairing_failed(
		&self,
		announced: &AnnouncedPairing,
		reason: String,
	) -> Result<(), String> {
		let session = responder_session_from_identity(&announced.identity, announced.peer)?;
		let failed = v2::EncryptedResponse::Failed(reason);
		submit_handshake_answer(
			&self.services,
			&session,
			announced.peer,
			&failed,
			"sso-responder pairing failure",
		)
		.await?;
		debug!("told pairing host that pairing failed");
		Ok(())
	}
}

fn responder_session(
	entropy: &[u8],
	network_suffix: &str,
	peer: PairedSsoPeer,
) -> Result<SsoSessionInfo, String> {
	let (identity, _) = derive_responder_identity(entropy, network_suffix)
		.map_err(|err| format!("responder identity derivation failed: {err}"))?;
	responder_session_from_identity(&identity, peer)
}

fn responder_session_from_identity(
	identity: &ResponderIdentity,
	peer: PairedSsoPeer,
) -> Result<SsoSessionInfo, String> {
	establish_responder_session_info(
		identity,
		peer.statement_account_id,
		peer.encryption_public_key,
	)
}

/// Encrypt `response` to the pairing host and post it on the handshake topic.
async fn submit_handshake_answer(
	services: &RuntimeServices,
	session: &SsoSessionInfo,
	peer: PairedSsoPeer,
	response: &v2::EncryptedResponse,
	context: &'static str,
) -> Result<(), String> {
	let handshake = encrypt_v2_handshake_response(peer.encryption_public_key, response)?;
	let topic = bootstrap_topic(peer.statement_account_id, peer.encryption_public_key);
	let statement = build_signed_statement(
		session,
		topic,
		topic,
		handshake.encode(),
		fresh_statement_expiry(),
	)?;
	resend_while_unreachable(context, Resend::Briefly, || {
		services.statement_store.submit(&statement, context)
	})
	.await
}

/// Keep the responder identity and peer for both pairing notices.
// No `Debug`: it holds the responder statement secret.
pub struct AnnouncedPairing {
	identity: ResponderIdentity,
	peer: PairedSsoPeer,
}

/// Serve inbound session statements until the session ends.
#[instrument(skip_all, fields(runtime.method = "sso_responder.serve_session"))]
async fn serve_session(
	responder: &SsoResponderService,
	session: SsoSessionInfo,
	replay_scope: SsoReplayScope,
) -> Result<ResponderExit, String> {
	let service = SigningHostSsoService::new(responder.signing_host.clone());
	let services = &responder.services;
	let rpc_client = services
		.statement_store
		.client("sso-responder session")
		.await
		.map_err(|err| err.to_string())?;
	let subscription =
		statement_store_rpc::subscribe_match_all(&rpc_client, &[session.session_id_peer])
			.await
			.map_err(|err| format!("sso-responder subscribe failed: {err}"))?;
	let session = &session;
	let pages = futures::stream::unfold(
		(subscription, DecodeFailureRequestIds::new()),
		move |(mut subscription, mut decode_failure_request_ids)| async move {
			let item = subscription.next().await?;
			let page = match item {
				Ok(value) => {
					read_statements(services, session, &mut decode_failure_request_ids, &value)
						.await
				},
				Err(err) => Err(format!("sso-responder subscription failed: {err}")),
			};
			Some((page, (subscription, decode_failure_request_ids)))
		},
	);
	// Boxed as a trait object so the hosts that await a session need not
	// lay out this future or prove it `Send`.
	serve_pages(pages, responder.signing_host.sso_withdrawals(), |incoming| {
		serve_statement(responder, &service, session, replay_scope, incoming)
	})
	.boxed()
	.await
}

/// Queued requests at which reading pauses. A page already read is queued
/// whole, so the queue can pass this by up to one page.
const MAX_QUEUED_REQUESTS: usize = 64;

/// Serve the requests in `pages` one at a time, reading on while each runs
/// so a `Cancel` reaches the request it withdraws.
async fn serve_pages<Fut>(
	pages: impl Stream<Item = Result<Vec<IncomingSsoRequest>, String>>,
	withdrawals: &SsoWithdrawals,
	mut serve: impl FnMut(IncomingSsoRequest) -> Fut,
) -> Result<ResponderExit, String>
where
	Fut: Future<Output = Result<Option<ResponderExit>, String>>,
{
	let pages = pages.fuse();
	let serving = Fuse::terminated();
	pin_mut!(pages, serving);
	let mut queue = VecDeque::new();
	let mut failure = None;
	loop {
		if serving.is_terminated() &&
			let Some(incoming) = queue.pop_front()
		{
			serving.set(serve(incoming).fuse());
		}
		// A full queue pauses reading, as serving each request inline did.
		if queue.len() >= MAX_QUEUED_REQUESTS {
			if let Some(exit) = serving.as_mut().await? {
				return Ok(exit);
			}
			continue;
		}
		futures::select! {
			exit = serving => {
				if let Some(exit) = exit? {
					return Ok(exit);
				}
			}
			page = pages.next() => {
				let page = match page {
					Some(Ok(page)) => page,
					Some(Err(reason)) => {
						failure = Some(reason);
						break;
					}
					None => break,
				};
				for incoming in page {
					if let Some(targets) = withdrawn_targets(&incoming) {
						for target in targets {
							withdrawals.withdraw(target);
						}
						continue;
					}
					log_request_received(&incoming);
					queue.push_back(incoming);
				}
			}
		}
	}
	// The running request has been acked and recorded as started, so it is
	// answered before the loop stops, whatever stopped it.
	if !serving.is_terminated() &&
		let Some(exit) = serving.await?
	{
		return Ok(exit);
	}
	if let Some(reason) = failure {
		return Err(reason);
	}
	for incoming in queue {
		if let Some(exit) = serve(incoming).await? {
			return Ok(exit);
		}
	}
	Ok(ResponderExit::SubscriptionEnded)
}

/// Decode one page of session statements into the requests it carries.
///
/// A statement whose envelope decodes but whose messages do not is acked as
/// `DecodingFailed` once, so the peer fails fast instead of waiting out its
/// response deadline.
async fn read_statements(
	services: &RuntimeServices,
	session: &SsoSessionInfo,
	decode_failure_request_ids: &mut DecodeFailureRequestIds,
	value: &serde_json::Value,
) -> Result<Vec<IncomingSsoRequest>, String> {
	let page = parse_new_statements_result("sso-responder".to_string(), value)
		.map_err(|err| err.to_string())?;
	let mut requests = Vec::new();
	for statement in page.statements {
		let incoming = match decode_incoming_sso_request(session, &statement) {
			Ok(Some(incoming)) => incoming,
			Ok(None) => continue,
			Err(error) => {
				let prefix = hex::encode(&statement[..statement.len().min(16)]);
				warn!(
					reason = %error.reason,
					statement_bytes = statement.len(),
					statement_prefix = %prefix,
					"ignoring undecodable SSO session statement"
				);
				if let Some(request_id) = error.request_id &&
					decode_failure_request_ids.insert(request_id.clone())
				{
					acknowledge_request(
						services,
						session,
						request_id,
						SsoResponseCode::DecodingFailed,
						"sso-responder decode-failed ack",
					)
					.await?;
				}
				continue;
			},
		};
		requests.push(incoming);
	}
	Ok(requests)
}

/// Record each message of a request the responder will serve.
fn log_request_received(incoming: &IncomingSsoRequest) {
	for message in &incoming.messages {
		let cli_summary = format!(
			"Incoming SSO request · {}\nstatement_request_id={}\nremote_message_id={}",
			message.name(),
			incoming.request_id,
			message.message_id
		);
		tracing::event!(
			target: "truapi::sso_transcript",
			tracing::Level::DEBUG,
			cli_summary = cli_summary.as_str(),
			cli_event = "request_received",
			request = message.name(),
			statement_request_id = %incoming.request_id,
			remote_message_id = %message.message_id,
		);
	}
}

/// The requests a statement withdraws, when it carries nothing but `Cancel`s.
fn withdrawn_targets(incoming: &IncomingSsoRequest) -> Option<Vec<&str>> {
	incoming
		.messages
		.iter()
		.map(|message| match &message.data {
			RemoteMessageData::V1(v1::RemoteMessage::Cancel(withdrawal)) => {
				Some(withdrawal.message_id.as_str())
			},
			_ => None,
		})
		.collect()
}

/// Serve one inbound request statement exactly once across redeliveries.
async fn serve_statement(
	responder: &SsoResponderService,
	service: &SigningHostSsoService,
	session: &SsoSessionInfo,
	replay_scope: SsoReplayScope,
	incoming: IncomingSsoRequest,
) -> Result<Option<ResponderExit>, String> {
	let services = &responder.services;
	let request_id = incoming.request_id.clone();
	let request =
		ReplayRequest { id: &request_id, expires_at_unix_secs: incoming.expires_at_unix_secs };
	let duplicate_exit = duplicate_request_exit(&incoming);
	let mut unpublished = Vec::new();
	let execution = execute_once(
		services.platform.as_ref(),
		&responder.replay_locks,
		replay_scope,
		request,
		statement_current_unix_secs(),
		acknowledge_request(
			services,
			session,
			request_id.clone(),
			SsoResponseCode::Success,
			"sso-responder ack",
		),
		|| serve_request(services, service, session, incoming, &mut unpublished),
	)
	.await;
	responder.unpublished_answers.keep(replay_scope, &request_id, unpublished);
	Ok(match execution? {
		ReplayExecution::Duplicate => {
			publish_kept_answers(responder, session, replay_scope, &request_id).await?;
			duplicate_exit
		},
		ReplayExecution::Executed(exit) => exit,
	})
}

/// Publish the answers kept for `request_id` when it is delivered again,
/// keeping those the People chain still refuses.
async fn publish_kept_answers(
	responder: &SsoResponderService,
	session: &SsoSessionInfo,
	replay_scope: SsoReplayScope,
	request_id: &str,
) -> Result<(), String> {
	let mut refused = Vec::new();
	let mut refusal = None;
	for answer in responder.unpublished_answers.take(replay_scope, request_id) {
		if let Err(reason) = publish_answer(&responder.services, session, &answer).await {
			refused.push(answer);
			refusal.get_or_insert(reason);
		}
	}
	responder.unpublished_answers.keep(replay_scope, request_id, refused);
	refusal.map_or(Ok(()), Err)
}

/// Answer the batched messages of one acked request statement.
///
/// An answer the People chain refuses is added to `unpublished` and the
/// remaining messages are still answered, since the request will not run
/// again. The first refusal is returned.
async fn serve_request(
	services: &RuntimeServices,
	service: &SigningHostSsoService,
	session: &SsoSessionInfo,
	incoming: IncomingSsoRequest,
	unpublished: &mut Vec<RemoteMessage>,
) -> Result<Option<ResponderExit>, String> {
	let mut refusal = None;
	for message in incoming.messages {
		let request_name = message.name();
		let responding_to = message.message_id.clone();
		let started = Instant::now();
		let (response, outcome) = match service.answer(message).await {
			Dispatch::Response(answer) => (answer.message, answer.outcome),
			Dispatch::Disconnected => {
				debug!("pairing host disconnected the SSO session");
				return Ok(Some(ResponderExit::PeerDisconnected));
			},
			Dispatch::NotARequest(name) => {
				warn!(name, "peer sent a response variant as a request");
				continue;
			},
			Dispatch::Withdraw(_) => continue,
			Dispatch::Withdrawn => {
				debug!(%responding_to, "pairing host withdrew the SSO request");
				continue;
			},
		};
		let response_message_id = response.message_id.clone();
		let publish_result = publish_answer(services, session, &response).await;
		let elapsed_ms = started.elapsed().as_millis();
		match publish_result {
			Ok(()) => {
				let cli_summary = response_cli_summary(
					"SSO response sent",
					request_name,
					&incoming.request_id,
					&responding_to,
					&response_message_id,
					&outcome,
					elapsed_ms,
				);
				tracing::event!(
					target: "truapi::sso_transcript",
					tracing::Level::DEBUG,
					cli_summary = cli_summary.as_str(),
					cli_event = "response_sent",
					request = request_name,
					statement_request_id = %incoming.request_id,
					responding_to = %responding_to,
					%response_message_id,
					outcome = outcome.outcome,
					reason = outcome.reason.as_deref().unwrap_or_default(),
					elapsed_ms = u64::try_from(elapsed_ms).unwrap_or(u64::MAX),
				);
			},
			Err(reason) => {
				let failure =
					ResponseOutcome { outcome: "publish_failed", reason: Some(reason.clone()) };
				let cli_summary = response_cli_summary(
					"SSO response failed",
					request_name,
					&incoming.request_id,
					&responding_to,
					&response_message_id,
					&failure,
					elapsed_ms,
				);
				tracing::event!(
					target: "truapi::sso_transcript",
					tracing::Level::WARN,
					cli_summary = cli_summary.as_str(),
					cli_event = "response_failed",
					request = request_name,
					statement_request_id = %incoming.request_id,
					responding_to = %responding_to,
					%response_message_id,
					outcome = failure.outcome,
					reason = %reason,
					elapsed_ms = u64::try_from(elapsed_ms).unwrap_or(u64::MAX),
				);
				unpublished.push(response);
				refusal.get_or_insert(reason);
			},
		}
	}
	refusal.map_or(Ok(None), Err)
}

/// Publish `answer` on the session's request channel, sending it again until
/// the People chain takes or refuses it.
async fn publish_answer(
	services: &RuntimeServices,
	session: &SsoSessionInfo,
	answer: &RemoteMessage,
) -> Result<(), String> {
	let statement = build_outgoing_request_statement(
		session,
		format!("resp:{}", answer.message_id),
		vec![answer.clone()],
		fresh_statement_expiry(),
	)?;
	publish(services, &statement, "sso-responder response", Resend::UntilAnswered).await
}

async fn acknowledge_request(
	services: &RuntimeServices,
	session: &SsoSessionInfo,
	request_id: String,
	code: SsoResponseCode,
	label: &'static str,
) -> Result<(), String> {
	let ack = build_signed_session_response_statement(
		session,
		request_id,
		code as u8,
		fresh_statement_expiry(),
	)?;
	publish(services, &ack, label, Resend::Briefly).await
}

/// Publish a session statement, resending it as `resend` allows.
async fn publish(
	services: &RuntimeServices,
	statement: &[u8],
	label: &'static str,
	resend: Resend,
) -> Result<(), String> {
	resend_while_unreachable(label, resend, || {
		services.statement_store.submit_sso(statement, label)
	})
	.await
}

/// Run `submit`, and again after a pause while the People chain is
/// unreachable, as long as `resend` allows. Any answer from the store is final.
/// Resending is safe: the store answers `known` for a statement it holds.
async fn resend_while_unreachable<SubmitFn, SubmitFuture>(
	label: &'static str,
	resend: Resend,
	submit: SubmitFn,
) -> Result<(), String>
where
	SubmitFn: Fn() -> SubmitFuture,
	SubmitFuture: Future<Output = Result<(), StatementSubmitError>>,
{
	let mut pauses = RESEND_PAUSES;
	let mut sends = 0;
	retry(
		|| {
			sends += 1;
			let can_resend = resend.allows_another_after(sends);
			let submitted = submit();
			async move {
				match submitted.await {
					Err(error) if can_resend && error.is_unreachable() => {
						ControlFlow::Continue(error)
					},
					result => ControlFlow::Break(result.map_err(|error| error.to_string())),
				}
			}
		},
		|error| {
			warn!(label, %error, "People chain unreachable; sending the statement again");
			pauses.next_pause()
		},
	)
	.await
}

fn duplicate_request_exit(incoming: &IncomingSsoRequest) -> Option<ResponderExit> {
	incoming
		.messages
		.iter()
		.any(|message| {
			matches!(&message.data, RemoteMessageData::V1(v1::RemoteMessage::Disconnected))
		})
		.then_some(ResponderExit::PeerDisconnected)
}

fn response_cli_summary(
	heading: &str,
	request_name: &str,
	statement_request_id: &str,
	responding_to: &str,
	response_message_id: &str,
	result: &ResponseOutcome,
	elapsed_ms: u128,
) -> String {
	let mut summary = format!(
		"{heading} · {request_name} · {}\nstatement_request_id={statement_request_id}\nresponding_to={responding_to}\nresponse_message_id={response_message_id}\nelapsed_ms={elapsed_ms}",
		result.outcome
	);
	if let Some(reason) = &result.reason {
		summary.push_str("\nreason=");
		summary.push_str(&reason.replace(['\r', '\n'], " "));
	}
	summary
}

#[cfg(test)]
mod tests;
