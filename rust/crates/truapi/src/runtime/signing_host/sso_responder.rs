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

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;

use core::future::Future;

use futures::future::{Fuse, FusedFuture};
use futures::{FutureExt, Stream, StreamExt, pin_mut};
use parity_scale_codec::Encode;
use tracing::{debug, instrument, warn};
use truapi::v01;

use super::sso_replay::{ReplayExecution, SsoReplayScope, execute_once};
use super::{SigningHost, SigningHostSsoService};
use crate::chain_runtime::RuntimeFailure;
use crate::host_internal::sso_messages::{
    IncomingSsoRequest, OnExistingAllowancePolicy, RemoteMessage, RemoteMessageData,
    SsoResponseCode, build_outgoing_request_statement, build_signed_session_response_statement,
    decode_incoming_sso_request, v1,
};
use crate::host_internal::sso_wire::ResponseOutcome;
use crate::host_logic::entropy::root_entropy_source;
use crate::host_logic::product_account::derive_sr25519_hard_path;
use crate::host_logic::product_account::{
    ProductAccountError, derive_identity_keypair, derive_root_keypair_from_entropy,
};
use crate::host_logic::session::SsoSessionInfo;
use crate::host_logic::sso::pairing::{
    ResponderIdentity, VersionedHandshakeProposal, bootstrap_topic, decode_pairing_deeplink,
    derive_identity_chat_private_key, derive_x25519_keypair_from_entropy,
    encrypt_v2_handshake_response, establish_responder_session_info, v2, x25519_public_key,
};
use crate::host_logic::statement_store::{build_signed_statement, parse_new_statements_result};
use crate::runtime::authority::{AuthorityError, AuthoritySession};
use crate::runtime::services::RuntimeServices;
use crate::runtime::sso_remote::{fresh_statement_expiry, sso_message_id};
use crate::runtime::sso_service::{Dispatch, SsoWithdrawals};
use crate::runtime::statement_allowance::StatementAllowanceError;
use crate::runtime::statement_store_rpc;
use crate::runtime::statement_store_rpc::StatementStoreRpcClientError;
use crate::unix_time::current_unix_secs as statement_current_unix_secs;

/// RFC-0022 domain for the responder's persistent SSO X25519 key.
const SSO_ENCRYPTION_DOMAIN: &[u8] = b"sso";
/// Leave the product runtime one minute to receive and process the SSO response
/// before its 300-second remote-authority deadline expires.
const BULLETIN_AUTHORIZATION_WAIT: std::time::Duration = std::time::Duration::from_secs(240);

/// Upper bound on undecodable request ids acknowledged within one serve loop.
const MAX_DECODE_FAILURE_REQUEST_IDS: usize = 1024;

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
        Self {
            seen: HashSet::new(),
            order: VecDeque::new(),
        }
    }

    /// Record `request_id`, returning `true` if it was not already served.
    /// Evicts the oldest id when the capacity is exceeded.
    fn insert(&mut self, request_id: String) -> bool {
        if !self.seen.insert(request_id.clone()) {
            return false;
        }
        self.order.push_back(request_id);
        if self.order.len() > MAX_DECODE_FAILURE_REQUEST_IDS
            && let Some(evicted) = self.order.pop_front()
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
    pub fn from_deeplink(deeplink: &str) -> Result<Self, String> {
        Ok(PairingProposal::from_deeplink(deeplink)?.peer)
    }
}

/// Longest metadata value a pairing prompt renders; the rest is dropped.
pub const MAX_PAIRING_METADATA_CHARS: usize = 512;

impl PairingProposal {
    /// Decode a pairing deeplink into the peer it advertises and the metadata
    /// a host prompts with.
    pub fn from_deeplink(deeplink: &str) -> Result<Self, String> {
        let VersionedHandshakeProposal::V2(proposal) =
            decode_pairing_deeplink(deeplink).map_err(|err| err.to_string())?;
        let mut metadata = PairingProposalMetadata::default();
        for v2::MetadataEntry(key, value) in proposal.metadata {
            let value = sanitize_pairing_metadata(value);
            match key {
                v2::MetadataKey::HostName => metadata.host_name = value,
                v2::MetadataKey::HostVersion => metadata.host_version = value,
                v2::MetadataKey::HostIcon => metadata.host_icon = value,
                v2::MetadataKey::PlatformType => metadata.platform_type = value,
                v2::MetadataKey::PlatformVersion => metadata.platform_version = value,
                v2::MetadataKey::Custom(_) => {}
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
            !character.is_control()
                && !matches!(
                    character,
                    '\u{061c}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
                )
        })
        .take(MAX_PAIRING_METADATA_CHARS)
        .collect::<String>();
    (!value.is_empty()).then_some(value)
}

/// Failure while deriving or allocating a Statement Store/Bulletin allowance.
#[derive(Debug, thiserror::Error)]
pub enum AllowanceAllocationError {
    /// Signing host session or authority state was unavailable.
    #[error("{0}")]
    Authority(#[from] AuthorityError),
    /// The host serves no chain for this role, so there is nothing to claim on.
    #[error("host serves no {chain} chain")]
    ChainNotServed {
        /// Role that could not be resolved.
        chain: &'static str,
    },
    /// Reading the host's chain set failed.
    #[error("supported chains: {0}")]
    SupportedChains(String),
    /// Product-account key derivation failed.
    #[error("{0}")]
    ProductAccount(#[from] ProductAccountError),
    /// Chain state, metadata, ring, slot, proof, or extrinsic allocation failed.
    #[error("{0}")]
    StatementAllowance(#[from] StatementAllowanceError),
    /// Runtime service could not open the required Statement Store RPC client.
    #[error("{0}")]
    StatementStoreRpcClient(#[from] StatementStoreRpcClientError),
    /// Runtime service could not open the required Bulletin RPC client.
    #[error("{context}: {source}")]
    ChainRpcClient {
        /// Client context, naming which chain failed.
        context: &'static str,
        /// Chain runtime failure.
        #[source]
        source: RuntimeFailure,
    },
    /// System time cannot be converted into a UNIX timestamp.
    #[error("system clock before UNIX epoch")]
    SystemClockBeforeUnixEpoch,
    /// The signing account is not in any personhood ring.
    #[error("signing account is not a personhood ring member; cannot grant {resource} allowance")]
    MissingPersonhoodMembership {
        /// Resource name.
        resource: &'static str,
    },
}

impl AllowanceAllocationError {
    pub fn into_authority_error(self) -> AuthorityError {
        match self {
            Self::Authority(err) => err,
            other => AuthorityError::Unavailable {
                reason: other.to_string(),
            },
        }
    }
}

/// Answer `deeplink` and serve the resulting SSO session until it ends.
#[instrument(skip_all, fields(runtime.method = "sso_responder.respond_to_pairing"))]
pub async fn respond_to_pairing(
    services: Arc<RuntimeServices>,
    signing_host: Arc<SigningHost>,
    deeplink: &str,
) -> Result<ResponderExit, String> {
    let established = establish_pairing_session(&services, &signing_host, deeplink).await?;
    serve_session(
        services,
        signing_host,
        established.session,
        established.replay_scope,
    )
    .await
}

/// Answer a pairing host's handshake without entering its long-lived serve loop.
pub async fn establish_pairing(
    services: Arc<RuntimeServices>,
    signing_host: Arc<SigningHost>,
    deeplink: &str,
) -> Result<(), String> {
    establish_pairing_session(&services, &signing_host, deeplink).await?;
    Ok(())
}

async fn establish_pairing_session(
    services: &RuntimeServices,
    signing_host: &SigningHost,
    deeplink: &str,
) -> Result<EstablishedPairing, String> {
    let peer = PairedSsoPeer::from_deeplink(deeplink)?;
    let entropy = signing_host
        .root_entropy()
        .map_err(|err| format!("signing host has no active local session: {err}"))?;
    // Product accounts and the SSO statement identity derive from the
    // canonical root key; the identity is the RFC-0022 `uid.<suffix>` default
    // account of the network this host is configured for.
    let root = derive_root_keypair_from_entropy(&entropy)
        .map_err(|err| format!("root account derivation failed: {err}"))?;
    let (identity, identity_chat_private_key) =
        derive_responder_identity(&entropy, signing_host.network_suffix())
            .map_err(|err| format!("responder identity derivation failed: {err}"))?;
    let device_enc_pub_key = x25519_public_key(services.device_encryption_secret().await?);
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
        services,
        &session,
        peer,
        &success,
        "sso-responder handshake",
    )
    .await?;
    debug!("answered pairing handshake");
    // The submit is the earliest point the peer could read the answer.
    if let Some(observer) = services.device_pairing_observer() {
        observer.device_paired(peer);
    }

    Ok(EstablishedPairing {
        session,
        replay_scope: SsoReplayScope {
            root_public_key: root.public.to_bytes(),
            peer_statement_account_id: peer.statement_account_id,
            peer_encryption_public_key: peer.encryption_public_key,
        },
    })
}

/// Resume a previously paired SSO session from its persisted public peer keys.
pub async fn resume_pairing(
    services: Arc<RuntimeServices>,
    signing_host: Arc<SigningHost>,
    peer: PairedSsoPeer,
) -> Result<ResponderExit, String> {
    let entropy = signing_host
        .root_entropy()
        .map_err(|err| format!("signing host has no active local session: {err}"))?;
    let root = derive_root_keypair_from_entropy(&entropy)
        .map_err(|err| format!("root account derivation failed: {err}"))?;
    let session = responder_session(&entropy, signing_host.network_suffix(), peer)?;
    serve_session(
        services,
        signing_host,
        session,
        SsoReplayScope {
            root_public_key: root.public.to_bytes(),
            peer_statement_account_id: peer.statement_account_id,
            peer_encryption_public_key: peer.encryption_public_key,
        },
    )
    .await
}

/// Notify a paired host that this signing host is ending their SSO session.
pub async fn disconnect_paired_host(
    services: Arc<RuntimeServices>,
    signing_host: Arc<SigningHost>,
    peer: PairedSsoPeer,
) -> Result<(), String> {
    let entropy = signing_host
        .root_entropy()
        .map_err(|err| format!("signing host has no active local session: {err}"))?;
    let session = responder_session(&entropy, signing_host.network_suffix(), peer)?;
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
    services
        .statement_store
        .submit_sso(statement, "sso-responder disconnect")
        .await
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
    services.statement_store.submit(statement, context).await
}

/// The identity and peer a pairing notice went out under.
///
/// Carries the responder secret rather than re-deriving it, because the signer
/// this host is allocating under can rotate between the two notices: the
/// account that sent the first one is the one already holding an allowance to
/// send the second.
// No `Debug`: it holds the responder statement secret.
pub struct AnnouncedPairing {
    identity: ResponderIdentity,
    peer: PairedSsoPeer,
}

/// Tell the pairing host that allowance allocation is under way, moving it off
/// its QR screen for however long the allocation takes.
///
/// Answering at all needs this host's own statement-store allowance, which is
/// the `WalletSso` renewal target, so callers register that before calling.
/// Callers own the matching [`notify_pairing_failed`]: the host has dropped its
/// QR and waits without a deadline, so an allocation that then fails leaves it
/// waiting forever unless it is told.
pub async fn notify_pairing_allowance_allocation(
    services: Arc<RuntimeServices>,
    signing_host: Arc<SigningHost>,
    deeplink: &str,
) -> Result<AnnouncedPairing, String> {
    let peer = PairedSsoPeer::from_deeplink(deeplink)?;
    let entropy = signing_host
        .root_entropy()
        .map_err(|err| format!("signing host has no active local session: {err}"))?;
    let (identity, _) = derive_responder_identity(&entropy, signing_host.network_suffix())
        .map_err(|err| format!("responder identity derivation failed: {err}"))?;
    let session = responder_session_from_identity(&identity, peer)?;

    let pending = v2::EncryptedResponse::Pending(v2::Status::AllowanceAllocation);
    submit_handshake_answer(
        &services,
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
    services: Arc<RuntimeServices>,
    announced: &AnnouncedPairing,
    reason: String,
) -> Result<(), String> {
    let session = responder_session_from_identity(&announced.identity, announced.peer)?;
    let failed = v2::EncryptedResponse::Failed(reason);
    submit_handshake_answer(
        &services,
        &session,
        announced.peer,
        &failed,
        "sso-responder pairing failure",
    )
    .await?;
    debug!("told pairing host that pairing failed");
    Ok(())
}

/// Serve inbound session statements until the session ends.
#[instrument(skip_all, fields(runtime.method = "sso_responder.serve_session"))]
async fn serve_session(
    services: Arc<RuntimeServices>,
    signing_host: Arc<SigningHost>,
    session: SsoSessionInfo,
    replay_scope: SsoReplayScope,
) -> Result<ResponderExit, String> {
    let service = SigningHostSsoService::new(signing_host.clone());
    let rpc_client = services
        .statement_store
        .client("sso-responder session")
        .await
        .map_err(|err| err.to_string())?;
    let subscription =
        statement_store_rpc::subscribe_match_all(&rpc_client, &[session.session_id_peer])
            .await
            .map_err(|err| format!("sso-responder subscribe failed: {err}"))?;
    let (services, session) = (&services, &session);
    let pages = futures::stream::unfold(
        (subscription, DecodeFailureRequestIds::new()),
        move |(mut subscription, mut decode_failure_request_ids)| async move {
            let item = subscription.next().await?;
            let page = match item {
                Ok(value) => {
                    read_statements(services, session, &mut decode_failure_request_ids, &value)
                        .await
                }
                Err(err) => Err(format!("sso-responder subscription failed: {err}")),
            };
            Some((page, (subscription, decode_failure_request_ids)))
        },
    );
    // Boxed as a trait object so the hosts that await a session need not
    // lay out this future or prove it `Send`.
    serve_pages(pages, signing_host.sso_withdrawals(), |incoming| {
        serve_statement(
            services,
            &signing_host,
            &service,
            session,
            replay_scope,
            incoming,
        )
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
        if serving.is_terminated()
            && let Some(incoming) = queue.pop_front()
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
    if !serving.is_terminated()
        && let Some(exit) = serving.await?
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
                if let Some(request_id) = error.request_id
                    && decode_failure_request_ids.insert(request_id.clone())
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
            }
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
            }
            _ => None,
        })
        .collect()
}

/// Serve one inbound request statement exactly once across redeliveries.
async fn serve_statement(
    services: &RuntimeServices,
    signing_host: &SigningHost,
    service: &SigningHostSsoService,
    session: &SsoSessionInfo,
    replay_scope: SsoReplayScope,
    incoming: IncomingSsoRequest,
) -> Result<Option<ResponderExit>, String> {
    let request_id = incoming.request_id.clone();
    let expires_at_unix_secs = incoming.expires_at_unix_secs;
    let duplicate_exit = duplicate_request_exit(&incoming);
    let execution = execute_once(
        services.platform.as_ref(),
        signing_host.sso_replay_locks(),
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
        }
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
            }
            Dispatch::NotARequest(name) => {
                warn!(name, "peer sent a response variant as a request");
                continue;
            }
            Dispatch::Withdraw(_) => continue,
            Dispatch::Withdrawn => {
                debug!(%responding_to, "pairing host withdrew the SSO request");
                continue;
            }
        };
        let response_message_id = response.message_id.clone();
        let statement_request_id = format!("resp:{response_message_id}");
        let statement = build_outgoing_request_statement(
            session,
            statement_request_id,
            vec![response],
            fresh_statement_expiry(),
        )?;
        let publish_result = services
            .statement_store
            .submit_sso(statement, "sso-responder response")
            .await;
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
                    elapsed_ms = elapsed_ms as u64,
                );
            }
            Err(reason) => {
                let failure = ResponseOutcome {
                    outcome: "publish_failed",
                    reason: Some(reason.clone()),
                };
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
                    elapsed_ms = elapsed_ms as u64,
                );
                return Err(reason);
            }
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
    services
        .statement_store
        .submit_sso(ack, "sso-responder ack")
        .await
}

fn duplicate_request_exit(incoming: &IncomingSsoRequest) -> Option<ResponderExit> {
    incoming
        .messages
        .iter()
        .any(|message| {
            matches!(
                &message.data,
                RemoteMessageData::V1(v1::RemoteMessage::Disconnected)
            )
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

/// A product's statement-store allowance key, and the period it holds a slot in.
pub struct StatementStoreAllocation {
    /// sr25519 secret of the product's allowance account.
    pub secret: Vec<u8>,
    /// Allowance period the slot was found or claimed in.
    pub period: u32,
}

pub async fn allocate_statement_store_allowance(
    services: &RuntimeServices,
    signing_host: &SigningHost,
    session: &AuthoritySession,
    product_id: &str,
    policy: OnExistingAllowancePolicy,
) -> Result<StatementStoreAllocation, AllowanceAllocationError> {
    use super::allowance_renewal::{self, StatementRenewalTarget};
    use crate::runtime::statement_allowance::{
        self, PooledRegistrationParams, allocated_in, find_including_rings,
        register_statement_account_pooled, scan_collections,
    };

    signing_host.require_current_session(session)?;
    let entropy = signing_host.root_entropy()?;
    let allowance =
        derive_sr25519_hard_path(&entropy, &["allowance", "statement-store", product_id])?;
    // The key is derived locally; only its registration needs the chain. A
    // host answering allocation as granted hands back the derived key so a
    // product can sign with it, and skips the registration, so nothing it
    // signs is accepted by a real statement store.
    #[cfg(feature = "test-host")]
    if signing_host.grants_allowances_unchecked() {
        return Ok(StatementStoreAllocation {
            secret: allowance.secret.to_bytes().to_vec(),
            period: statement_allowance::slot::current_period(current_unix_secs()?),
        });
    }
    let target = allowance.public.to_bytes();
    let candidates = signing_host.reserved_person_collection_candidates(session)?;
    let client = services
        .statement_store
        .chain_client("statement-store allowance")
        .await?;
    let rpc = client.rpc();
    let chain = services.chain_context.get(&client).await?;
    let network_suffix = statement_allowance::slot::read_network_suffix(rpc).await?;
    let period = statement_allowance::slot::current_period(current_unix_secs()?);
    let reuse_existing = matches!(policy, OnExistingAllowancePolicy::Ignore);

    // Held from the scan through the submission, not just around the submission:
    // the scan is what picks the free slot, so a renewal pass scanning in the gap
    // would choose the same one. Released on the early return below, which
    // submits nothing.
    let _registration = signing_host.renewal.registration_lock().lock().await;

    // One read of the period's slot tables, reused below rather than rescanned:
    // when an allowance is already recorded on chain neither a proof nor a
    // submission is needed, and a ring snapshot pages in every member key.
    let scans = scan_collections(
        rpc,
        &chain.metadata,
        &candidates,
        &network_suffix,
        period,
        &target,
        reuse_existing,
    )
    .await?;
    if let Some((collection, seq)) = allocated_in(&scans) {
        debug!(
            %product_id,
            period,
            seq,
            %collection,
            "statement-store allowance already allocated"
        );
        signing_host.require_current_session(session)?;
        return Ok(StatementStoreAllocation {
            secret: allowance.secret.to_bytes().to_vec(),
            period,
        });
    }

    // Every ring back to index 0, because a membership that stopped being
    // re-included still proves against the ring that holds it.
    let memberships = find_including_rings(rpc, &chain.metadata, &candidates, u32::MAX).await?;
    if memberships.is_empty() {
        return Err(AllowanceAllocationError::MissingPersonhoodMembership {
            resource: "statement-store",
        });
    }
    signing_host.require_current_session(session)?;
    let outcome = register_statement_account_pooled(
        rpc,
        &chain.metadata,
        &chain.state,
        &scans,
        &memberships,
        PooledRegistrationParams {
            target: &target,
            period,
            network_suffix: &network_suffix,
            reuse_existing,
            // Connecting a product must not revoke another product's allowance.
            // A full period is reported as exhaustion; reclaiming space is the
            // renewal pass's job, which only ever replaces for its own ledger.
            allow_eviction: false,
            protected: &[],
        },
    )
    .await?;
    match outcome {
        statement_allowance::RegistrationOutcome::Registered {
            block_hash,
            seq,
            ring_index,
            collection,
        } => {
            debug!(
                %product_id,
                %block_hash,
                seq,
                ring_index,
                %collection,
                "registered statement-store allowance"
            );
        }
        statement_allowance::RegistrationOutcome::AlreadyAllocated { seq, collection } => {
            debug!(
                %product_id,
                seq,
                %collection,
                "statement-store allowance already allocated"
            );
        }
    }
    signing_host.require_current_session(session)?;
    if let Err(reason) = allowance_renewal::track(
        signing_host,
        vec![StatementRenewalTarget::ProductStatementAllowance {
            product_id: product_id.to_string(),
        }],
    )
    .await
    {
        warn!(%product_id, %reason, "failed to record statement-store renewal target");
    }
    signing_host.require_current_session(session)?;
    Ok(StatementStoreAllocation {
        secret: allowance.secret.to_bytes().to_vec(),
        period,
    })
}

pub async fn allocate_bulletin_allowance(
    services: &RuntimeServices,
    signing_host: &SigningHost,
    session: &AuthoritySession,
    product_id: &str,
    policy: OnExistingAllowancePolicy,
) -> Result<Vec<u8>, AllowanceAllocationError> {
    use crate::runtime::statement_allowance::collection::PersonhoodCollection;
    use crate::runtime::statement_allowance::{
        self, claim_long_term_storage, fetch_bulletin_allowance, find_including_rings,
        wait_bulletin_authorization,
    };

    signing_host.require_current_session(session)?;
    let entropy = signing_host.root_entropy()?;
    let allowance = derive_sr25519_hard_path(&entropy, &["allowance", "bulletin", product_id])?;
    #[cfg(feature = "test-host")]
    if signing_host.grants_allowances_unchecked() {
        return Ok(allowance.secret.to_bytes().to_vec());
    }
    let target = allowance.public.to_bytes();

    let bulletin_rpc = statement_allowance::rpc::RpcClient::new(
        services
            .bulletin
            .client("bulletin allowance")
            .await
            .map_err(|source| AllowanceAllocationError::ChainRpcClient {
                context: "bulletin allowance client",
                source,
            })?,
    );
    let current_allowance = fetch_bulletin_allowance(&bulletin_rpc, &target).await?;
    if matches!(policy, OnExistingAllowancePolicy::Ignore)
        && current_allowance.is_some_and(|allowance| allowance.available())
    {
        signing_host.require_current_session(session)?;
        return Ok(allowance.secret.to_bytes().to_vec());
    }

    let people_client = services
        .statement_store
        .chain_client("bulletin allowance claim")
        .await?;
    let people_rpc = people_client.rpc();
    let chain = services.chain_context.get(&people_client).await?;
    let network_suffix = statement_allowance::slot::read_network_suffix(people_rpc).await?;
    let candidates = signing_host.reserved_person_collection_candidates(session)?;
    // Statement-store slots and PGAS claims are each bounded by a per-collection
    // constant, so their budgets are meant to be spent per collection. Long-term
    // storage is bounded by `Resources.LongTermStorageClaimsPerPeriod` alone, with
    // no per-collection variant, so the budget reads as per person. Its spent
    // counters are still keyed by a collection-scoped alias, which means changing
    // collection silently restarts the count at zero. Staying in the light
    // collection keeps one person to one count; full personhood is the fallback
    // for a device without light personhood.
    let memberships =
        find_including_rings(people_rpc, &chain.metadata, &candidates, u32::MAX).await?;
    let membership = memberships
        .iter()
        .find(|membership| membership.collection() == PersonhoodCollection::LitePeople)
        .or_else(|| memberships.first())
        .ok_or(AllowanceAllocationError::MissingPersonhoodMembership {
            resource: "Bulletin",
        })?;
    let period_duration =
        statement_allowance::slot::long_term_storage_period_duration(&chain.metadata)?;
    let period = statement_allowance::slot::current_long_term_storage_period(
        current_unix_secs()?,
        period_duration,
    )?;
    signing_host.require_current_session(session)?;
    let outcome = claim_long_term_storage(statement_allowance::LongTermStorageClaim {
        rpc: people_rpc,
        metadata: &chain.metadata,
        chain_state: &chain.state,
        entropy: membership.entropy,
        network_suffix: &network_suffix,
        target: &target,
        period,
        ring: &membership.ring,
    })
    .await?;
    let statement_allowance::LongTermStorageOutcome::Claimed {
        block_hash,
        counter,
        ring_index,
    } = outcome;
    debug!(
        %product_id,
        %block_hash,
        counter,
        ring_index,
        "claimed Bulletin long-term storage allowance"
    );

    let authorization = wait_bulletin_authorization(
        &bulletin_rpc,
        &target,
        current_allowance,
        BULLETIN_AUTHORIZATION_WAIT,
    )
    .await?;
    debug!(
        %product_id,
        remained_size = authorization.remained_size,
        remained_transactions = authorization.remained_transactions,
        "Bulletin authorization visible"
    );
    signing_host.require_current_session(session)?;
    Ok(allowance.secret.to_bytes().to_vec())
}

/// Claim an Asset Hub PGAS allowance for the product account `derivation_index`
/// selects.
///
/// Unlike the statement-store and Bulletin allowances, this credits the product
/// account itself rather than a dedicated `//allowance//…` account, and returns
/// nothing: PGAS pre-warms a balance on an account the host already controls, so
/// there is no key to hand back.
///
/// Asset Hub is resolved through the host's chain set rather than a configured
/// hash, so a host that does not serve it says so instead of claiming against
/// whatever chain a stale hash happens to reach.
pub async fn allocate_smart_contract_allowance(
    services: &RuntimeServices,
    signing_host: &SigningHost,
    session: &AuthoritySession,
    product_id: &str,
    derivation_index: v01::DerivationIndex,
    policy: OnExistingAllowancePolicy,
) -> Result<(), AllowanceAllocationError> {
    use truapi::latest::ChainIdentifier;

    use crate::host_logic::features;
    use crate::runtime::statement_allowance::{self, ChainClient, find_including_rings, pgas};

    signing_host.require_current_session(session)?;

    // PGAS credits the product account the caller named.
    let target = signing_host
        .product_keypair(&v01::ProductAccountId {
            dot_ns_identifier: product_id.to_string(),
            derivation_index,
        })?
        .public
        .to_bytes();

    let chains = features::supported_chains(services.platform.as_ref())
        .await
        .map_err(|err| AllowanceAllocationError::SupportedChains(err.reason))?;
    let asset_hub_genesis = features::genesis_for(&chains, ChainIdentifier::AssetHub)
        .ok_or(AllowanceAllocationError::ChainNotServed { chain: "Asset Hub" })?;
    let asset_hub_client = ChainClient::new(
        statement_allowance::rpc::RpcClient::new(subxt_rpcs::RpcClient::new(
            services
                .chain
                .rpc_client("PGAS allowance", &asset_hub_genesis)
                .await
                .map_err(|source| AllowanceAllocationError::ChainRpcClient {
                    context: "Asset Hub PGAS client",
                    source,
                })?,
        )),
        asset_hub_genesis,
    );
    let asset_hub = services.chain_context.get(&asset_hub_client).await?;

    // A claim spends one of the day's slots, so honour a caller that asked to leave
    // an existing allowance alone rather than topping up an already-warm account.
    if matches!(policy, OnExistingAllowancePolicy::Ignore)
        && pgas::holds_a_full_claim(asset_hub_client.rpc(), &asset_hub.metadata, &target).await?
    {
        debug!(%product_id, "PGAS allowance already funded; leaving it alone");
        signing_host.require_current_session(session)?;
        return Ok(());
    }
    let network_suffix =
        statement_allowance::slot::read_network_suffix(asset_hub_client.rpc()).await?;

    let people_client = services
        .statement_store
        .chain_client("PGAS allowance ring")
        .await?;
    let people_rpc = people_client.rpc();
    let people = services.chain_context.get(&people_client).await?;

    let candidates = signing_host.reserved_person_collection_candidates(session)?;
    // A single claim needs one collection, so take the strongest membership the
    // person actually holds rather than assuming light personhood.
    let membership = find_including_rings(people_rpc, &people.metadata, &candidates, u32::MAX)
        .await?
        .into_iter()
        .next()
        .ok_or(AllowanceAllocationError::MissingPersonhoodMembership { resource: "PGAS" })?;

    signing_host.require_current_session(session)?;
    let outcome = pgas::claim_pgas(pgas::PgasClaim {
        asset_hub_rpc: asset_hub_client.rpc(),
        asset_hub: &asset_hub,
        people_rpc,
        people_metadata: &people.metadata,
        entropy: membership.entropy,
        network_suffix: &network_suffix,
        target: &target,
        ring: &membership.ring,
    })
    .await?;
    debug!(
        %product_id,
        day = outcome.day,
        slot_index = outcome.slot_index,
        ring_index = outcome.ring_index,
        block = %outcome.block_hash,
        "claimed PGAS allowance"
    );
    signing_host.require_current_session(session)?;
    Ok(())
}

/// Wall-clock seconds since the UNIX epoch, used to pick the allowance period.
///
/// `std::time::SystemTime` compiles for wasm32 but panics when read, so the
/// browser takes its clock from `web-time` instead.
pub fn current_unix_secs() -> Result<u64, AllowanceAllocationError> {
    #[cfg(not(target_arch = "wasm32"))]
    use std::time::{SystemTime, UNIX_EPOCH};
    #[cfg(target_arch = "wasm32")]
    use web_time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| AllowanceAllocationError::SystemClockBeforeUnixEpoch)
}

#[cfg(test)]
mod tests {
    use super::super::LocalActivation;
    use super::*;
    use crate::host_internal::sso_messages::{
        self, GetAccountAliasResponse, RemoteMessage, RingVrfError, SsoAllocatedResource,
        SsoAllocationOutcome,
    };
    use crate::host_internal::sso_wire::ResponseOutcome;
    use crate::host_logic::product_account::derive_ring_vrf_domain_entropy;

    /// The key a host advertises on chain must be the one it serves over
    /// pairing. These derive independently, so a test that asks only one of
    /// them cannot see the two drift apart — which is the failure this pins.
    #[test]
    fn the_advertised_chat_key_is_the_one_the_responder_serves() {
        const CHAT_ENTROPY: [u8; 16] = [0xAB; 16];
        const SUFFIX: &str = "paseo";

        let (_, served) =
            derive_responder_identity(&CHAT_ENTROPY, SUFFIX).expect("responder identity derives");
        let registration = crate::host_logic::attestation::build_lite_registration(
            &CHAT_ENTROPY,
            SUFFIX,
            [0x11; 32],
            "headless",
            None,
            0,
        )
        .expect("registration builds");

        assert_eq!(
            &registration.identifier_key[1..33],
            &crate::host_logic::sso::pairing::x25519_public_key(served),
            "the advertised chat key is not the one served over pairing"
        );
    }
    use crate::host_logic::statement_store::decode_verified_statement_data;
    use crate::platform::{HostInfo, Platform, PlatformInfo, SigningHostConfig};
    use crate::runtime::authority::ProductAuthority;
    use crate::runtime::services::RuntimeServices;
    use crate::test_support::{StubPlatform, test_spawner};
    use std::sync::Arc;
    use truapi::latest as api;

    const ENTROPY: [u8; 16] = [0xab; 16];
    /// The fixture's People chain is paseo-next-v2 (see `PEOPLE_METADATA`),
    /// whose runtime carries the `paseo` network suffix.
    const NETWORK_SUFFIX: &str = "paseo";

    fn signing_fixture(platform: Arc<StubPlatform>) -> (Arc<RuntimeServices>, Arc<SigningHost>) {
        let platform: Arc<dyn Platform> = platform;
        let config = SigningHostConfig::new(
            HostInfo {
                name: "Polkadot Mobile".to_string(),
                icon: None,
                version: None,
                platform: truapi::latest::HostPlatform::Unknown,
            },
            PlatformInfo::default(),
            [0; 32],
            [0xbb; 32],
            [0xcc; 32],
            NETWORK_SUFFIX.to_string(),
        )
        .expect("signing host config is valid");
        let services = RuntimeServices::new(
            platform.clone(),
            config.host.host_info.clone(),
            config.people_chain_genesis_hash,
            config.bulletin_chain_genesis_hash,
            config.asset_hub_chain_genesis_hash,
            test_spawner(),
        );
        let signing_host = SigningHost::new(services.clone(), config.network_suffix);
        futures::executor::block_on(signing_host.activate_local_session(ENTROPY.to_vec()))
            .expect("activation succeeds");
        (services, signing_host)
    }

    /// Metadata for the People chain the signing fixture is configured for.
    #[cfg(not(target_arch = "wasm32"))]
    const PEOPLE_METADATA: &[u8] =
        include_bytes!("../../../tests/fixtures/paseo-next-v2-metadata-v16.scale");

    /// An existing statement-store allowance must be served without resolving a
    /// ring or submitting anything. The cache and the scan are covered on their
    /// own; this pins the composition, so removing the early return fails here
    /// rather than passing quietly.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn an_existing_allowance_is_served_without_touching_the_ring() {
        use futures::FutureExt;

        use crate::host_logic::product_account::{
            derive_full_person_ring_vrf_entropy, derive_sr25519_hard_path,
        };
        use crate::runtime::statement_allowance::slot;

        let product_id = "myapp.dot";
        let allowance =
            derive_sr25519_hard_path(&ENTROPY, &["allowance", "statement-store", product_id])
                .expect("allowance derivation succeeds");
        // The scan reads slot 0 first; answering it with an entry naming the
        // allowance account is the "already allocated" case.
        let slot_entry = (allowance.public.to_bytes(), 0u32, 0u64).encode();
        let people_row = slot::testing::slot_row(
            derive_full_person_ring_vrf_entropy(&ENTROPY, NETWORK_SUFFIX),
            NETWORK_SUFFIX.as_bytes(),
            slot::current_period(current_unix_secs().unwrap()),
            &[Some(format!(r#""0x{}""#, hex::encode(&slot_entry)))],
        );

        // Keyed by method, not by request order: this path decodes ~450 KiB of
        // metadata between two requests, which outruns the ordered script's
        // fixed-poll pump on a loaded runner.
        let platform = Arc::new(StubPlatform {
            rpc_method_responses: vec![
                (
                    "state_getRuntimeVersion",
                    r#"{"specVersion":1000000,"transactionVersion":1}"#.to_string(),
                ),
                (
                    "chain_getBlockHash",
                    format!(r#""0x{}""#, hex::encode([0u8; 32])),
                ),
                (
                    "Metadata_metadata_at_version",
                    format!(
                        r#""0x{}""#,
                        hex::encode(Some(PEOPLE_METADATA.to_vec()).encode()),
                    ),
                ),
                // The scan bound, read through the `Resources` view functions.
                (
                    "RuntimeViewFunction_execute_view_function",
                    format!(
                        r#""0x{}""#,
                        hex::encode(Ok::<Vec<u8>, ()>(20u32.encode()).encode()),
                    ),
                ),
                (
                    "RuntimeViewFunction_execute_view_function",
                    format!(
                        r#""0x{}""#,
                        hex::encode(Ok::<Vec<u8>, ()>(10u32.encode()).encode()),
                    ),
                ),
                // The network suffix, read once before the scan.
                (
                    "state_getStorage",
                    format!(r#""0x{}""#, hex::encode(b"paseo".to_vec().encode())),
                ),
                (
                    "state_queryStorageAt",
                    people_row,
                ),
                // The LitePeople row, read alongside People's, is empty.
                (
                    "state_queryStorageAt",
                    r#"[{"block":"0xb10c","changes":[]}]"#.to_string(),
                ),
            ],
            ..Default::default()
        });
        let (services, signing_host) = signing_fixture(platform.clone());

        // Bounded, because the failure mode of losing the early return is a
        // wait on a chain read the stub deliberately does not answer — an
        // unbounded test would hang instead of reporting. The bound is generous
        // because it is catching a hang, not asserting latency.
        let allocation = futures::executor::block_on(async {
            let session = signing_host.current_session().unwrap();
            futures::select! {
                result = allocate_statement_store_allowance(
                    &services,
                    &signing_host,
                    &session,
                    product_id,
                    OnExistingAllowancePolicy::Ignore,
                )
                .fuse() => result,
                _ = futures_timer::Delay::new(std::time::Duration::from_secs(30)).fuse() => {
                    panic!("allocation blocked on a chain read it should not have made")
                }
            }
        })
        .expect("an existing allowance is returned");

        assert_eq!(allocation.secret, allowance.secret.to_bytes().to_vec());

        let sent = platform.sent_rpc.lock().expect("rpc list mutex poisoned");
        let methods: Vec<String> = sent
            .iter()
            .filter_map(|request| {
                let value: serde_json::Value = serde_json::from_str(request).ok()?;
                value.get("method")?.as_str().map(ToString::to_string)
            })
            .collect();

        // `find_including_rings` opens with `chain_getFinalizedHead`, so none of
        // these means no ring was resolved.
        assert_eq!(
            methods
                .iter()
                .filter(|method| *method == "chain_getFinalizedHead")
                .count(),
            0,
            "a ring was resolved for an allowance already in place: {methods:?}"
        );
        assert!(
            !methods
                .iter()
                .any(|method| method.starts_with("author_submit")),
            "an extrinsic was submitted for an allowance already in place: {methods:?}"
        );
    }

    #[test]
    fn responder_advertises_and_signs_with_the_local_uid_identity() {
        let (_services, signing_host) = signing_fixture(Arc::new(StubPlatform::default()));
        let local_identity = signing_host
            .current_session()
            .unwrap()
            .identity_account_id
            .unwrap();
        let (identity, _) = derive_responder_identity(&ENTROPY, NETWORK_SUFFIX).unwrap();
        assert_eq!(identity.statement_public_key, local_identity);
        // The statement identity is the network's `uid.<suffix>` account, the
        // one the pairing host resolves a username for; a `.dot` account has
        // no lite record on a test network.
        assert_ne!(
            derive_responder_identity(&ENTROPY, "dot")
                .unwrap()
                .0
                .statement_public_key,
            local_identity
        );

        let (_, host_encryption_public_key) =
            derive_x25519_keypair_from_entropy(&[0x42; 16], b"sso");
        let session =
            establish_responder_session_info(&identity, [0x55; 32], host_encryption_public_key)
                .unwrap();
        let statement = build_signed_statement(
            &session,
            [0x66; 32],
            [0x77; 32],
            b"handshake".to_vec(),
            fresh_statement_expiry(),
        )
        .unwrap();
        let verified =
            decode_verified_statement_data(&statement, Some(identity.statement_public_key))
                .unwrap();
        assert_eq!(verified.signer, local_identity);
    }

    #[test]
    fn advertised_device_key_is_independent_of_the_identity() {
        let (services, _signing_host) = signing_fixture(Arc::new(StubPlatform::default()));

        let advertised = x25519_public_key(
            futures::executor::block_on(services.device_encryption_secret()).unwrap(),
        );

        // The regression this guards: advertising the SSO channel key as the
        // device key makes every device sharing an identity indistinguishable.
        let (_, sso_public) = derive_x25519_keypair_from_entropy(&ENTROPY, SSO_ENCRYPTION_DOMAIN);
        assert_ne!(advertised, sso_public);
    }

    /// Both handshake notices the signing host sends outside `Success` must be
    /// readable by the host that advertised the keys, or the pairing host is
    /// left on a screen no later message can move it off.
    #[test]
    fn the_pairing_notices_decode_on_the_pairing_host() {
        use crate::host_logic::sso::pairing::{
            VersionedHandshakeResponse, decrypt_v2_handshake_response,
            generate_pairing_device_identity,
        };

        // Encrypt to the key a pairing host actually advertises, so each notice
        // is decrypted with the secret that host kept rather than one the test
        // chose for both sides.
        let pairing_identity = generate_pairing_device_identity().unwrap();
        let peer = PairedSsoPeer {
            statement_account_id: pairing_identity.statement_store_public_key,
            encryption_public_key: pairing_identity.encryption_public_key,
        };

        for response in [
            v2::EncryptedResponse::Pending(v2::Status::AllowanceAllocation),
            v2::EncryptedResponse::Failed("no free StatementStore slot".to_string()),
        ] {
            let VersionedHandshakeResponse::V2 {
                encrypted_message,
                public_key,
            } = encrypt_v2_handshake_response(peer.encryption_public_key, &response).unwrap();

            assert_eq!(
                decrypt_v2_handshake_response(
                    pairing_identity.encryption_secret_key,
                    public_key,
                    &encrypted_message,
                )
                .unwrap(),
                response,
            );
        }
    }

    fn pairing_deeplink(peer: PairedSsoPeer) -> String {
        let proposal = VersionedHandshakeProposal::V2(v2::Proposal {
            device: v2::Device {
                statement_account_id: peer.statement_account_id,
                encryption_public_key: peer.encryption_public_key,
            },
            metadata: vec![v2::MetadataEntry(
                v2::MetadataKey::HostName,
                "paired host".to_string(),
            )],
        });
        format!(
            "polkadotapp://pair?handshake={}",
            hex::encode(proposal.encode())
        )
    }

    #[derive(Default)]
    struct RecordingPairingObserver {
        paired: std::sync::Mutex<Vec<PairedSsoPeer>>,
    }

    impl RecordingPairingObserver {
        fn paired(&self) -> Vec<PairedSsoPeer> {
            self.paired
                .lock()
                .expect("paired device list mutex poisoned")
                .clone()
        }
    }

    impl DevicePairingObserver for RecordingPairingObserver {
        fn device_paired(&self, device: PairedSsoPeer) {
            self.paired
                .lock()
                .expect("paired device list mutex poisoned")
                .push(device);
        }
    }

    fn pairing_fixture(submit_status: &'static str) -> (Arc<RuntimeServices>, Arc<SigningHost>) {
        signing_fixture(Arc::new(StubPlatform {
            rpc_method_responses: vec![(
                "statement_submit",
                format!(r#"{{"status":"{submit_status}"}}"#),
            )],
            ..Default::default()
        }))
    }

    /// Without this the host never learns a device paired, so no contact is
    /// told a new device joined.
    #[test]
    fn a_paired_device_is_reported_to_the_host() {
        let peer = PairedSsoPeer {
            statement_account_id: [0x31; 32],
            encryption_public_key: x25519_public_key([0x42; 32]),
        };
        let (services, signing_host) = pairing_fixture("new");
        let observer = Arc::new(RecordingPairingObserver::default());
        assert!(services.install_device_pairing_observer(observer.clone()));

        futures::executor::block_on(establish_pairing(
            services,
            signing_host,
            &pairing_deeplink(peer),
        ))
        .expect("the handshake is answered");

        assert_eq!(observer.paired(), vec![peer]);
    }

    /// Reporting a handshake the peer never received would have the host
    /// announce a device over a session that does not exist.
    #[test]
    fn a_handshake_that_never_landed_reports_no_device() {
        let peer = PairedSsoPeer {
            statement_account_id: [0x31; 32],
            encryption_public_key: x25519_public_key([0x42; 32]),
        };
        // Neither "new" nor "known", so the submit is rejected.
        let (services, signing_host) = pairing_fixture("ignored");
        let observer = Arc::new(RecordingPairingObserver::default());
        assert!(services.install_device_pairing_observer(observer.clone()));

        let failure = futures::executor::block_on(establish_pairing(
            services,
            signing_host,
            &pairing_deeplink(peer),
        ))
        .expect_err("a rejected handshake submit fails the pairing");
        // Without this the test would also pass on a failure from earlier,
        // leaving the rule unexercised.
        assert!(
            failure.contains("statement_submit not accepted"),
            "pairing failed before the handshake submit: {failure}"
        );

        assert_eq!(observer.paired(), Vec::new());
    }

    #[test]
    fn pairing_without_an_observer_still_answers_the_handshake() {
        let peer = PairedSsoPeer {
            statement_account_id: [0x31; 32],
            encryption_public_key: x25519_public_key([0x42; 32]),
        };
        let (services, signing_host) = pairing_fixture("new");

        futures::executor::block_on(establish_pairing(
            services,
            signing_host,
            &pairing_deeplink(peer),
        ))
        .expect("the handshake is answered without an observer");
    }

    /// The peer writes what the pairing prompt calls it. Rendering that as
    /// sent lets it reorder or pad out the prompt's own words around itself.
    #[test]
    fn proposal_metadata_is_sanitized_before_a_host_prompts_with_it() {
        let long = "n".repeat(MAX_PAIRING_METADATA_CHARS + 10);
        let proposal = VersionedHandshakeProposal::V2(v2::Proposal {
            device: v2::Device {
                statement_account_id: [0x31; 32],
                encryption_public_key: [0x42; 32],
            },
            metadata: vec![
                // A right-to-left override and a newline, either of which
                // rewrites what the surrounding prompt text reads as.
                v2::MetadataEntry(
                    v2::MetadataKey::HostName,
                    "  Wallet\u{202e}trebo\n  ".to_string(),
                ),
                v2::MetadataEntry(v2::MetadataKey::HostVersion, long.clone()),
                v2::MetadataEntry(v2::MetadataKey::HostIcon, "\u{2066}\t ".to_string()),
                v2::MetadataEntry(v2::MetadataKey::Custom("seat".to_string()), "3".to_string()),
            ],
        });
        let deeplink = format!(
            "polkadotapp://pair?handshake={}",
            hex::encode(proposal.encode())
        );

        assert_eq!(
            PairingProposal::from_deeplink(&deeplink)
                .expect("a well-formed deeplink decodes")
                .metadata,
            PairingProposalMetadata {
                host_name: Some("Wallettrebo".to_string()),
                host_version: Some("n".repeat(MAX_PAIRING_METADATA_CHARS)),
                // Nothing renderable survived, so the host has no name to show
                // rather than an empty one.
                host_icon: None,
                // Neither key was sent, and a key outside the well-known set
                // is not one a prompt has a place for.
                platform_type: None,
                platform_version: None,
            }
        );
    }

    #[test]
    fn pairing_deeplink_exposes_the_public_material_needed_to_resume() {
        let proposal = VersionedHandshakeProposal::V2(v2::Proposal {
            device: v2::Device {
                statement_account_id: [0x31; 32],
                encryption_public_key: [0x42; 32],
            },
            metadata: vec![v2::MetadataEntry(
                v2::MetadataKey::HostName,
                "paired host".to_string(),
            )],
        });
        let deeplink = format!(
            "polkadotapp://pair?handshake={}",
            hex::encode(proposal.encode())
        );

        assert_eq!(
            PairedSsoPeer::from_deeplink(&deeplink).unwrap(),
            PairedSsoPeer {
                statement_account_id: [0x31; 32],
                encryption_public_key: [0x42; 32],
            }
        );
    }

    #[test]
    fn persisted_peer_rebuilds_the_original_responder_session() {
        let peer = PairedSsoPeer {
            statement_account_id: [0x53; 32],
            encryption_public_key: x25519_public_key([0x64; 32]),
        };
        let (identity, _) = derive_responder_identity(&ENTROPY, NETWORK_SUFFIX).unwrap();
        let mut expected = establish_responder_session_info(
            &identity,
            peer.statement_account_id,
            peer.encryption_public_key,
        )
        .unwrap();
        let resumed = responder_session(&ENTROPY, NETWORK_SUFFIX, peer).unwrap();

        assert_eq!(
            crate::host_logic::statement_store::statement_public_key_from_secret(resumed.ss_secret)
                .unwrap(),
            expected.ss_public_key
        );
        expected.ss_secret = resumed.ss_secret;

        assert_eq!(resumed, expected);
    }

    #[test]
    fn replayed_disconnect_still_terminates_the_peer() {
        let disconnect = IncomingSsoRequest {
            request_id: "disconnect-1".to_string(),
            expires_at_unix_secs: Some(200),
            messages: vec![RemoteMessage {
                message_id: "message-1".to_string(),
                data: RemoteMessageData::V1(v1::RemoteMessage::Disconnected),
            }],
        };
        let ordinary = IncomingSsoRequest {
            request_id: "empty-1".to_string(),
            expires_at_unix_secs: Some(200),
            messages: Vec::new(),
        };

        assert_eq!(
            (
                duplicate_request_exit(&disconnect),
                duplicate_request_exit(&ordinary)
            ),
            (Some(ResponderExit::PeerDisconnected), None)
        );
    }

    fn answer(
        signing_host: &Arc<SigningHost>,
        message_id: &str,
        request: v1::RemoteMessage,
    ) -> v1::RemoteMessage {
        let service = SigningHostSsoService::new(signing_host.clone());
        let message = RemoteMessage {
            message_id: message_id.to_string(),
            data: RemoteMessageData::V1(request),
        };
        let Dispatch::Response(answer) = futures::executor::block_on(service.answer(message))
        else {
            panic!("expected a response");
        };
        let RemoteMessageData::V1(data) = answer.message.data;
        data
    }

    #[test]
    fn dispatch_without_session_distinguishes_requests_responses_and_disconnects() {
        let (_, signing_host) = signing_fixture(Arc::new(StubPlatform::default()));
        let service = SigningHostSsoService::new(signing_host);
        let dispatch = |data| {
            futures::executor::block_on(service.dispatch(
                None,
                RemoteMessage {
                    message_id: "m-1".to_string(),
                    data: RemoteMessageData::V1(data),
                },
            ))
        };

        assert_eq!(
            dispatch(v1::RemoteMessage::Disconnected),
            Dispatch::Disconnected
        );
        assert_eq!(
            dispatch(v1::RemoteMessage::ProductSubtreeResponse(
                sso_messages::Response {
                    responding_to: "m-1".to_string(),
                    payload: Ok([7; 32]),
                }
            )),
            Dispatch::NotARequest("ProductSubtreeResponse"),
        );
        let Dispatch::Response(answer) = dispatch(v1::RemoteMessage::ProductSubtreeRequest(
            sso_messages::ProductSubtreeRequest {
                product_id: "myapp.dot".to_string(),
            },
        )) else {
            panic!("expected a disconnected error response");
        };
        let RemoteMessageData::V1(v1::RemoteMessage::ProductSubtreeResponse(response)) =
            answer.message.data
        else {
            panic!("expected the request's response variant");
        };
        assert_eq!(response.responding_to, "m-1");
        assert_eq!(
            response.payload,
            Err("signing host session is not active".to_string())
        );
    }

    #[test]
    fn response_summary_reports_protocol_errors_without_multiline_output() {
        let payload: GetAccountAliasResponse = Err(RingVrfError::Unknown {
            reason: "chain RPC\ntimed out".to_string(),
        });

        let result = ResponseOutcome::from_payload(&payload);
        let summary = response_cli_summary(
            "SSO response sent",
            "get_account_alias",
            "statement-1",
            "alias-1",
            "alias-1:response",
            &result,
            42,
        );

        assert_eq!(result.outcome, "error");
        assert_eq!(
            summary,
            "SSO response sent · get_account_alias · error\n\
             statement_request_id=statement-1\n\
             responding_to=alias-1\n\
             response_message_id=alias-1:response\n\
             elapsed_ms=42\n\
            reason=Unknown: chain RPC timed out"
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn allocation_failure_details_reach_the_response_transcript() {
        let (_, signing_host) = signing_fixture(Arc::new(StubPlatform {
            resource_allocation_confirmed: true,
            chain_connect_error: Some("allocation node unavailable"),
            ..StubPlatform::default()
        }));
        let service = SigningHostSsoService::new(signing_host);
        let request = RemoteMessage::request(
            "allocation-1".to_string(),
            sso_messages::ResourceAllocationRequest {
                calling_product_id: "myapp.dot".to_string(),
                resources: vec![
                    api::AllocatableResource::AutoSigning,
                    api::AllocatableResource::BulletinAllowance,
                    api::AllocatableResource::StatementStoreAllowance,
                ],
                on_existing: OnExistingAllowancePolicy::Ignore,
            },
        );
        let Dispatch::Response(answer) = futures::executor::block_on(service.answer(request))
        else {
            panic!("expected an allocation response");
        };
        let RemoteMessageData::V1(v1::RemoteMessage::ResourceAllocationResponse(response)) =
            answer.message.data
        else {
            panic!("expected an allocation response");
        };
        assert!(matches!(
            response.payload.unwrap().as_slice(),
            [
                SsoAllocationOutcome::Allocated(SsoAllocatedResource::AutoSigning { .. }),
                SsoAllocationOutcome::NotAvailable,
                SsoAllocationOutcome::NotAvailable,
            ]
        ));
        assert_eq!(answer.outcome.outcome, "partial");
        let reason = answer.outcome.reason.as_deref().unwrap();
        assert!(reason.starts_with("1 of 3 requested resources allocated; 2 unavailable"));
        assert_eq!(
            reason.matches("allocation node unavailable").count(),
            2,
            "{reason}"
        );
        assert!(!reason.contains(['\r', '\n']));
        let cli = response_cli_summary(
            "SSO response sent",
            "resource_allocation",
            "allocation-1",
            "allocation-1",
            &answer.message.message_id,
            &answer.outcome,
            0,
        );
        assert!(cli.contains(&format!("reason={reason}")));
    }

    #[test]
    fn account_alias_requires_confirmation_for_cross_product_request() {
        let (_, signing_host) = signing_fixture(Arc::new(StubPlatform::default()));

        let response = answer(
            &signing_host,
            "alias-1",
            v1::RemoteMessage::GetAccountAliasRequest(sso_messages::ProductRequest {
                calling_product_id: "myapp.dot".to_string(),
                payload: api::HostAccountGetAliasRequest {
                    key_handle: api::ProductAccountId {
                        dot_ns_identifier: "peopl.dot".to_string(),
                        derivation_index: api::DerivationIndex::Index(0),
                    },
                    context: api::ProductProofContext {
                        product_id: "other.dot".to_string(),
                        suffix: api::DerivationIndex::Index(0),
                    },
                    ring_location: api::RingLocation {
                        chain_id: [0; 32],
                        junctions: vec![],
                    },
                },
            }),
        );

        let v1::RemoteMessage::GetAccountAliasResponse(response) = response else {
            panic!("expected alias response");
        };
        assert_eq!(response.payload.unwrap_err(), RingVrfError::Rejected);
    }

    #[test]
    fn resource_allocation_requires_confirmation_before_allocation() {
        let platform = Arc::new(StubPlatform::default());
        let (_, signing_host) = signing_fixture(platform.clone());

        let response = answer(
            &signing_host,
            "alloc-1",
            v1::RemoteMessage::ResourceAllocationRequest(sso_messages::ResourceAllocationRequest {
                calling_product_id: "myapp.dot".to_string(),
                resources: vec![api::AllocatableResource::StatementStoreAllowance],
                on_existing: sso_messages::OnExistingAllowancePolicy::Ignore,
            }),
        );

        let v1::RemoteMessage::ResourceAllocationResponse(response) = response else {
            panic!("expected resource allocation response");
        };
        assert_eq!(
            response.payload.unwrap(),
            vec![SsoAllocationOutcome::Rejected]
        );

        // The confirmation review names the beneficiary product so the user
        // knows which product receives the delegated allowance key.
        let reviews = platform
            .resource_allocation_reviews
            .lock()
            .expect("resource allocation review list mutex poisoned");
        assert_eq!(reviews.len(), 1);
        assert_eq!(reviews[0].calling_product_id, "myapp.dot");
    }

    #[test]
    fn auto_signing_allocation_returns_the_product_subtree_secret() {
        let platform = Arc::new(StubPlatform {
            resource_allocation_confirmed: true,
            ..StubPlatform::default()
        });
        let (_, signing_host) = signing_fixture(platform);
        let expected_secret = signing_host
            .product_subtree_secret("myapp.dot")
            .expect("product subtree secret derives");
        let expected_ring_vrf_domain_entropy =
            derive_ring_vrf_domain_entropy(&ENTROPY, "myapp.dot")
                .expect("ring-VRF domain entropy derives");

        let response = answer(
            &signing_host,
            "alloc-auto-signing",
            v1::RemoteMessage::ResourceAllocationRequest(sso_messages::ResourceAllocationRequest {
                calling_product_id: "myapp.dot".to_string(),
                resources: vec![api::AllocatableResource::AutoSigning],
                on_existing: sso_messages::OnExistingAllowancePolicy::Ignore,
            }),
        );

        let v1::RemoteMessage::ResourceAllocationResponse(response) = response else {
            panic!("expected resource allocation response");
        };
        assert_eq!(
            response.payload.unwrap(),
            vec![SsoAllocationOutcome::Allocated(
                SsoAllocatedResource::AutoSigning {
                    product_root_private_key: expected_secret,
                    ring_vrf_domain_entropy: expected_ring_vrf_domain_entropy,
                }
            )]
        );
    }

    fn allocation_after_session_change(replacement: Option<Vec<u8>>) {
        use futures::{FutureExt, channel::oneshot};

        let (release, gate) = oneshot::channel();
        let platform = Arc::new(StubPlatform {
            resource_allocation_confirmed: true,
            resource_allocation_confirmation_gate: std::sync::Mutex::new(Some(gate)),
            ..StubPlatform::default()
        });
        let (_, signing_host) = signing_fixture(platform.clone());
        let service = SigningHostSsoService::new(signing_host.clone());
        let message = RemoteMessage::request(
            "alloc-stale".to_string(),
            sso_messages::ResourceAllocationRequest {
                calling_product_id: "myapp.dot".to_string(),
                resources: vec![api::AllocatableResource::AutoSigning],
                on_existing: OnExistingAllowancePolicy::Ignore,
            },
        );

        futures::executor::block_on(async {
            let answer = service.answer(message);
            futures::pin_mut!(answer);
            assert!(answer.as_mut().now_or_never().is_none());
            assert_eq!(
                platform.resource_allocation_reviews.lock().unwrap().len(),
                1
            );

            signing_host.disconnect().await;
            if let Some(entropy) = replacement {
                signing_host.activate_local_session(entropy).await.unwrap();
            }
            release.send(()).unwrap();

            let Dispatch::Response(answer) = answer.await else {
                panic!("expected an allocation response");
            };
            let RemoteMessageData::V1(v1::RemoteMessage::ResourceAllocationResponse(response)) =
                answer.message.data
            else {
                panic!("expected an allocation response");
            };
            assert_eq!(response.responding_to, "alloc-stale");
            assert!(
                response.payload.is_err(),
                "stale consent must not release keys"
            );
            assert!(platform.sent_rpc.lock().unwrap().is_empty());
        });
    }

    fn allocation_request(message_id: &str) -> RemoteMessage {
        RemoteMessage::request(
            message_id.to_string(),
            sso_messages::ResourceAllocationRequest {
                calling_product_id: "myapp.dot".to_string(),
                resources: vec![api::AllocatableResource::AutoSigning],
                on_existing: OnExistingAllowancePolicy::Ignore,
            },
        )
    }

    fn cancel(message_id: &str, target: &str) -> RemoteMessage {
        RemoteMessage {
            message_id: message_id.to_string(),
            data: RemoteMessageData::V1(v1::RemoteMessage::Cancel(sso_messages::Withdrawal {
                message_id: target.to_string(),
            })),
        }
    }

    /// The phone spends once the person approves an allocation, so a
    /// withdrawal that arrives while the prompt is open must end the prompt
    /// with nothing allocated and nothing to post.
    #[test]
    fn a_cancel_withdraws_an_allocation_while_its_prompt_is_open() {
        let (_release, gate) = futures::channel::oneshot::channel();
        let platform = Arc::new(StubPlatform {
            resource_allocation_confirmed: true,
            resource_allocation_confirmation_gate: std::sync::Mutex::new(Some(gate)),
            ..StubPlatform::default()
        });
        let (_, signing_host) = signing_fixture(platform.clone());
        let service = SigningHostSsoService::new(signing_host);
        let allocation = service.answer(allocation_request("alloc-1"));
        futures::pin_mut!(allocation);
        assert!(allocation.as_mut().now_or_never().is_none());

        let withdrawal = service
            .answer(cancel("cancel-1", "alloc-1"))
            .now_or_never()
            .expect("a withdrawal is answered at once");
        let allocation = allocation
            .as_mut()
            .now_or_never()
            .expect("a withdrawn request stops waiting on its prompt");

        assert_eq!(
            (
                withdrawal,
                allocation,
                platform.sent_rpc.lock().unwrap().len()
            ),
            (
                Dispatch::Withdraw("alloc-1".to_string()),
                Dispatch::Withdrawn,
                0
            )
        );
    }

    /// Statements arrive out of order, so a withdrawal can land first. The
    /// request it names must then never reach the person at all.
    #[test]
    fn a_cancel_that_arrives_first_stops_its_request_before_it_prompts() {
        let platform = Arc::new(StubPlatform {
            resource_allocation_confirmed: true,
            ..StubPlatform::default()
        });
        let (_, signing_host) = signing_fixture(platform.clone());
        let service = SigningHostSsoService::new(signing_host);

        futures::executor::block_on(service.answer(cancel("cancel-1", "alloc-1")));
        let allocation = futures::executor::block_on(service.answer(allocation_request("alloc-1")));

        assert_eq!(allocation, Dispatch::Withdrawn);
        assert!(
            platform
                .resource_allocation_reviews
                .lock()
                .unwrap()
                .is_empty()
        );
    }

    fn statement(request_id: &str, message: RemoteMessage) -> Vec<IncomingSsoRequest> {
        vec![IncomingSsoRequest {
            request_id: request_id.to_string(),
            expires_at_unix_secs: None,
            messages: vec![message],
        }]
    }

    /// Without this a `Cancel` waits behind the very request it withdraws,
    /// and reaches it only after the person has answered.
    #[test]
    fn a_cancel_is_read_while_the_request_it_names_is_being_served() {
        let withdrawals = SsoWithdrawals::default();
        let (pages, statements) = futures::channel::mpsc::unbounded();
        pages
            .unbounded_send(Ok(statement("stmt-1", allocation_request("alloc-1"))))
            .unwrap();
        let serving = serve_pages(statements, &withdrawals, |incoming| {
            let request = withdrawals.begin(&incoming.messages[0].message_id);
            async move {
                if let Some(request) = request {
                    request.cancel.cancelled().await;
                }
                Ok(Some(ResponderExit::PeerDisconnected))
            }
        });
        futures::pin_mut!(serving);
        assert!(serving.as_mut().now_or_never().is_none());

        pages
            .unbounded_send(Ok(statement("stmt-2", cancel("cancel-1", "alloc-1"))))
            .unwrap();

        assert_eq!(
            serving.as_mut().now_or_never(),
            Some(Ok(ResponderExit::PeerDisconnected))
        );
    }

    /// The request being served has been acked and recorded as started, so
    /// redelivery would never answer it. A broken subscription must let it
    /// finish before the loop reports the failure.
    #[test]
    fn a_failing_subscription_lets_the_request_being_served_finish() {
        let withdrawals = SsoWithdrawals::default();
        let (pages, statements) = futures::channel::mpsc::unbounded();
        let (release, gate) = futures::channel::oneshot::channel::<()>();
        let mut gate = Some(gate);
        pages
            .unbounded_send(Ok(statement("stmt-1", allocation_request("alloc-1"))))
            .unwrap();
        let serving = serve_pages(statements, &withdrawals, |_| {
            let gate = gate.take();
            async move {
                if let Some(gate) = gate {
                    let _ = gate.await;
                }
                Ok(None)
            }
        });
        futures::pin_mut!(serving);
        assert!(serving.as_mut().now_or_never().is_none());

        pages
            .unbounded_send(Err("subscription failed".to_string()))
            .unwrap();
        let before_the_request_finished = serving.as_mut().now_or_never();
        let once_it_finished = before_the_request_finished.is_none().then(|| {
            let _ = release.send(());
            serving.as_mut().now_or_never()
        });

        assert_eq!(
            (before_the_request_finished, once_it_finished),
            (None, Some(Some(Err("subscription failed".to_string()))))
        );
    }

    #[test]
    fn resource_consent_cannot_authorize_a_replacement_account() {
        allocation_after_session_change(Some(vec![0xcd; 16]));
    }

    #[test]
    fn resource_consent_cannot_survive_same_account_reactivation() {
        allocation_after_session_change(Some(ENTROPY.to_vec()));
    }

    #[test]
    fn resource_consent_cannot_survive_disconnect() {
        allocation_after_session_change(None);
    }

    #[test]
    fn legacy_transaction_request_uses_the_controlled_identity_account() {
        let (_, signing_host) = signing_fixture(Arc::new(StubPlatform {
            create_transaction_confirmed: true,
            chain_connect_error: Some("fixture has no live chain"),
            ..StubPlatform::default()
        }));
        let identity = derive_identity_keypair(&ENTROPY, NETWORK_SUFFIX).unwrap();
        let payload = api::LegacyAccountTxPayload {
            signer: identity.public.to_bytes(),
            genesis_hash: [0xaa; 32],
            call_data: vec![0x00, 0x00],
            extensions: vec![api::TxPayloadExtension {
                id: "CheckNonce".to_string(),
                extra: vec![1],
                additional_signed: vec![2, 3],
            }],
            tx_ext_version: 0,
        };

        let response = answer(
            &signing_host,
            "legacy-tx-1",
            v1::RemoteMessage::CreateTransactionWithLegacyAccountRequest(
                sso_messages::CreateTransactionWithLegacyAccountRequest {
                    payload: sso_messages::CreateTransactionLegacyPayload::V1(payload),
                },
            ),
        );

        let v1::RemoteMessage::CreateTransactionResponse(response) = response else {
            panic!("expected create transaction response");
        };
        // The identity key is accepted, so the request gets as far as reading
        // the runtime metadata.
        let error = response.payload.expect_err("fixture has no chain metadata");
        assert!(
            format!("{error:?}").contains("cannot load chain metadata"),
            "{error:?}"
        );
    }

    #[test]
    fn product_subtree_request_is_consent_free_and_hard_derived() {
        let (_, signing_host) = signing_fixture(Arc::new(StubPlatform::default()));
        let response = answer(
            &signing_host,
            "subtree-1",
            v1::RemoteMessage::ProductSubtreeRequest(sso_messages::ProductSubtreeRequest {
                product_id: "browse.dot".to_string(),
            }),
        );

        let v1::RemoteMessage::ProductSubtreeResponse(response) = response else {
            panic!("expected product subtree response");
        };
        let root =
            derive_root_keypair_from_entropy(&ENTROPY).expect("fixture entropy derives root");
        let expected =
            crate::host_logic::product_account::derive_product_subtree_keypair(&root, "browse.dot")
                .expect("fixture derives subtree")
                .public
                .to_bytes();
        assert_eq!(response.responding_to, "subtree-1");
        assert_eq!(response.payload, Ok(expected));
    }

    #[test]
    fn decode_failure_request_ids_dedup_and_bound() {
        let mut served = DecodeFailureRequestIds::new();

        // First sighting is served; an immediate duplicate is rejected.
        assert!(served.insert("req-a".to_string()));
        assert!(!served.insert("req-a".to_string()));

        // Fill to capacity with distinct ids; the set never exceeds the bound.
        for i in 0..MAX_DECODE_FAILURE_REQUEST_IDS {
            served.insert(format!("fill-{i}"));
        }
        assert_eq!(served.seen.len(), MAX_DECODE_FAILURE_REQUEST_IDS);
        assert_eq!(served.order.len(), MAX_DECODE_FAILURE_REQUEST_IDS);

        // The oldest id ("req-a") has been evicted, so it is accepted again,
        // while a recent id is still deduped — memory stays bounded regardless
        // of how many ids a peer streams.
        assert!(served.insert("req-a".to_string()));
        assert!(!served.insert(format!("fill-{}", MAX_DECODE_FAILURE_REQUEST_IDS - 1)));
        assert_eq!(served.seen.len(), MAX_DECODE_FAILURE_REQUEST_IDS);
    }
}
