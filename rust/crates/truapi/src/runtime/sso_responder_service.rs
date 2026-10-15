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
	collections::{HashSet, VecDeque},
	sync::Arc,
	time::Instant,
};

use core::future::Future;

use futures::{
	FutureExt, Stream, StreamExt,
	future::{Fuse, FusedFuture},
	pin_mut,
};
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
		session::SsoSessionInfo,
		sso::pairing::{
			PairingDeeplinkDecodeError, VersionedHandshakeProposal, decode_pairing_deeplink, v2,
		},
		statement_store::parse_new_statements_result,
		x25519_seal::X25519PrivateKey,
	},
	runtime::{
		authority::{AccountHolder, AuthoritySession},
		services::RuntimeServices,
		signing_host::{SigningHost, SigningHostSsoService, WalletAccountHolder},
		sso_remote::{fresh_statement_expiry, sso_message_id},
		sso_service::{Dispatch, SsoWithdrawals},
		statement_store_rpc,
	},
	unix_time::current_unix_secs as statement_current_unix_secs,
};
use replay::{ReplayExecution, SsoReplayLocks, SsoReplayScope, execute_once};

/// Upper bound on undecodable request ids acknowledged within one serve loop.
const MAX_DECODE_FAILURE_REQUEST_IDS: usize = 1024;

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
	wallet: Arc<WalletAccountHolder>,
	signing_host: Arc<SigningHost>,
	/// Serializes replay-ledger updates within each wallet and peer scope.
	replay_locks: SsoReplayLocks,
}

impl SsoResponderService {
	/// Answer peers of `signing_host` over the runtime's statement store.
	pub fn new(services: Arc<RuntimeServices>, signing_host: Arc<SigningHost>) -> Arc<Self> {
		Arc::new(Self {
			services,
			wallet: signing_host.wallet().clone(),
			signing_host,
			replay_locks: SsoReplayLocks::default(),
		})
	}

	/// The wallet activation a pairing operation answers under.
	fn wallet_session(&self) -> Result<AuthoritySession, String> {
		self.wallet
			.current_session()
			.ok_or_else(|| "signing host has no active local session".to_string())
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
		let wallet_session = self.wallet_session()?;
		let device_enc_secret =
			X25519PrivateKey::from(self.services.device_encryption_secret().await?);
		let device_enc_pub_key = device_enc_secret.public_key().0;
		let (session, statement) = self
			.wallet
			.pairing_answer(&wallet_session, peer, device_enc_pub_key)
			.map_err(|error| error.to_string())?;
		self.services
			.statement_store
			.submit(statement, "sso-responder handshake")
			.await?;
		self.wallet
			.require_current_session(&wallet_session)
			.map_err(|error| error.to_string())?;
		debug!("answered pairing handshake");
		// The submit is the earliest point the peer could read the answer.
		if let Some(observer) = self.services.device_pairing_observer() {
			observer.device_paired(peer);
		}

		Ok(EstablishedPairing {
			session,
			replay_scope: SsoReplayScope {
				root_public_key: wallet_session.public_key,
				peer_statement_account_id: peer.statement_account_id,
				peer_encryption_public_key: peer.encryption_public_key,
			},
		})
	}

	/// Resume a previously paired SSO session from its persisted public peer keys.
	pub async fn resume_pairing(&self, peer: PairedSsoPeer) -> Result<ResponderExit, String> {
		let wallet_session = self.wallet_session()?;
		let session = self
			.wallet
			.sso_session(&wallet_session, peer)
			.map_err(|error| error.to_string())?;
		serve_session(
			self,
			session,
			SsoReplayScope {
				root_public_key: wallet_session.public_key,
				peer_statement_account_id: peer.statement_account_id,
				peer_encryption_public_key: peer.encryption_public_key,
			},
		)
		.await
	}

	/// Notify a paired host that this signing host is ending their SSO session.
	pub async fn disconnect_paired_host(&self, peer: PairedSsoPeer) -> Result<(), String> {
		let wallet_session = self.wallet_session()?;
		let session = self
			.wallet
			.sso_session(&wallet_session, peer)
			.map_err(|error| error.to_string())?;
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
		self.services
			.statement_store
			.submit_sso(statement, "sso-responder disconnect")
			.await
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
		let wallet_session = self.wallet_session()?;
		let pending = v2::EncryptedResponse::Pending(v2::Status::AllowanceAllocation);
		let statement = self
			.wallet
			.pairing_notice(&wallet_session, peer, &pending)
			.map_err(|error| error.to_string())?;
		self.services
			.statement_store
			.submit(statement, "sso-responder allowance allocation")
			.await?;
		debug!("told pairing host that allowance allocation started");
		Ok(AnnouncedPairing { wallet: self.wallet.clone(), session: wallet_session, peer })
	}

	/// Tell the pairing host that pairing failed, so it reports `reason` and offers
	/// a retry rather than waiting on an answer that is never coming.
	pub async fn notify_pairing_failed(
		&self,
		announced: &AnnouncedPairing,
		reason: String,
	) -> Result<(), String> {
		let statement = announced
			.wallet
			.pairing_notice(
				&announced.session,
				announced.peer,
				&v2::EncryptedResponse::Failed(reason),
			)
			.map_err(|error| error.to_string())?;
		self.services
			.statement_store
			.submit(statement, "sso-responder pairing failure")
			.await?;
		debug!("told pairing host that pairing failed");
		Ok(())
	}
}

/// Wallet activation and peer bound to an in-progress pairing notice.
pub struct AnnouncedPairing {
	wallet: Arc<WalletAccountHolder>,
	session: AuthoritySession,
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
					let ack = build_signed_session_response_statement(
						session,
						request_id,
						SsoResponseCode::DecodingFailed as u8,
						fresh_statement_expiry(),
					)?;
					services
						.statement_store
						.submit_sso(ack, "sso-responder decode-failed ack")
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
	let request_id = incoming.request_id.clone();
	let expires_at_unix_secs = incoming.expires_at_unix_secs;
	let duplicate_exit = duplicate_request_exit(&incoming);
	let services = &responder.services;
	let execution = execute_once(
		services.platform.as_ref(),
		&responder.replay_locks,
		replay_scope,
		&request_id,
		expires_at_unix_secs,
		statement_current_unix_secs(),
		|| serve_request(services, service, session, incoming),
	)
	.await?;
	Ok(match execution {
		ReplayExecution::Duplicate => {
			acknowledge_request(services, session, &request_id).await?;
			duplicate_exit
		},
		ReplayExecution::Executed(exit) => exit,
	})
}

/// Ack one inbound request statement and answer its batched messages.
async fn serve_request(
	services: &RuntimeServices,
	service: &SigningHostSsoService,
	session: &SsoSessionInfo,
	incoming: IncomingSsoRequest,
) -> Result<Option<ResponderExit>, String> {
	acknowledge_request(services, session, &incoming.request_id).await?;

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
		let statement_request_id = format!("resp:{response_message_id}");
		let statement = build_outgoing_request_statement(
			session,
			statement_request_id,
			vec![response],
			fresh_statement_expiry(),
		)?;
		let publish_result =
			services.statement_store.submit_sso(statement, "sso-responder response").await;
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
				return Err(reason);
			},
		}
	}
	Ok(None)
}

async fn acknowledge_request(
	services: &RuntimeServices,
	session: &SsoSessionInfo,
	request_id: &str,
) -> Result<(), String> {
	let ack = build_signed_session_response_statement(
		session,
		request_id.to_string(),
		SsoResponseCode::Success as u8,
		fresh_statement_expiry(),
	)?;
	services.statement_store.submit_sso(ack, "sso-responder ack").await
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
