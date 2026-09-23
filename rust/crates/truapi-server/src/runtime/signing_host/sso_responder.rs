//! Signing-host responder half of the host-spec §B pairing protocol.
//!
//! Answers a pairing host's handshake proposal (QR/deeplink) with an
//! encrypted `Success` statement, then serves the encrypted SSO session:
//! acks every inbound request statement, dispatches the batched
//! [`v1::RemoteMessage`] requests onto the local signing authority, and posts
//! the response statements the pairing host is waiting for. Runs until the
//! peer sends `Disconnected`, the local session ends, or the transport fails.
//!
//! Sensitive operations consult [`truapi_platform::UserConfirmation`], the
//! same seam browser hosts use for their confirmation modals; a headless host
//! implements it with its approval policy.

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;

use parity_scale_codec::Encode;
use tracing::{debug, instrument, warn};
use truapi::v01;

use super::sso_replay::{ReplayExecution, SsoReplayScope, execute_once};
use super::{SigningHost, SigningHostSsoService};
use crate::chain_runtime::RuntimeFailure;
use crate::host_logic::entropy::root_entropy_source;
use crate::host_logic::product_account::derive_sr25519_hard_path;
use crate::host_logic::product_account::{
    ProductAccountError, derive_identity_keypair, derive_root_keypair_from_entropy,
};
use crate::host_logic::session::SsoSessionInfo;
use crate::host_logic::sso::messages::{
    IncomingSsoRequest, OnExistingAllowancePolicy, RemoteMessage, RemoteMessageData,
    SsoResponseCode, build_outgoing_request_statement, build_signed_session_response_statement,
    decode_incoming_sso_request, v1,
};
use crate::host_logic::sso::pairing::{
    ResponderIdentity, VersionedHandshakeProposal, bootstrap_topic, decode_pairing_deeplink,
    derive_identity_chat_private_key, derive_x25519_keypair_from_entropy,
    encrypt_v2_handshake_response, establish_responder_session_info, v2, x25519_public_key,
};
use crate::host_logic::sso::wire::ResponseOutcome;
use crate::host_logic::statement_store::{
    build_signed_statement, current_unix_secs as statement_current_unix_secs,
    parse_new_statements_result,
};
use crate::runtime::authority::{AuthorityError, AuthoritySession};
use crate::runtime::services::RuntimeServices;
use crate::runtime::sso_remote::{fresh_statement_expiry, sso_message_id};
use crate::runtime::sso_service::Dispatch;
use crate::runtime::statement_allowance::StatementAllowanceError;
use crate::runtime::statement_store_rpc;
use crate::runtime::statement_store_rpc::StatementStoreRpcClientError;

/// RFC-0022 domain for the responder's persistent SSO X25519 key.
const SSO_ENCRYPTION_DOMAIN: &[u8] = b"sso";
/// Leave the product runtime one minute to receive and process the SSO response
/// before its 300-second remote-authority deadline expires.
const BULLETIN_AUTHORIZATION_WAIT: std::time::Duration = std::time::Duration::from_secs(240);

/// Upper bound on undecodable request ids acknowledged within one serve loop.
const MAX_DECODE_FAILURE_REQUEST_IDS: usize = 1024;

pub(super) fn derive_responder_identity(
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
pub enum ResponderExit {
    /// The pairing host announced `Disconnected`; its durable pairing may be removed.
    PeerDisconnected,
    /// The statement subscription ended without a disconnect message; retain the pairing and retry.
    SubscriptionEnded,
}

/// Public key material identifying one pairing host's resumable SSO session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PairedSsoPeer {
    /// Pairing host's statement-store account id.
    pub statement_account_id: [u8; 32],
    /// Pairing host's X25519 public key.
    pub encryption_public_key: [u8; 32],
}

struct EstablishedPairing {
    session: SsoSessionInfo,
    replay_scope: SsoReplayScope,
}

impl PairedSsoPeer {
    /// Extract the public peer material carried by a pairing deeplink.
    pub fn from_deeplink(deeplink: &str) -> Result<Self, String> {
        let VersionedHandshakeProposal::V2(proposal) =
            decode_pairing_deeplink(deeplink).map_err(|err| err.to_string())?;
        Ok(Self {
            statement_account_id: proposal.device.statement_account_id,
            encryption_public_key: proposal.device.encryption_public_key,
        })
    }
}

/// Failure while deriving or allocating a Statement Store/Bulletin allowance.
#[derive(Debug, thiserror::Error)]
pub(super) enum AllowanceAllocationError {
    /// Signing host session or authority state was unavailable.
    #[error("{0}")]
    Authority(#[from] AuthorityError),
    /// The host serves no chain for this role, so there is nothing to claim on.
    #[cfg(not(target_arch = "wasm32"))]
    #[error("host serves no {chain} chain")]
    ChainNotServed {
        /// Role that could not be resolved.
        chain: &'static str,
    },
    /// Reading the host's chain set failed.
    #[cfg(not(target_arch = "wasm32"))]
    #[error("supported chains: {0}")]
    SupportedChains(String),
    #[error("{0}")]
    ProductAccount(#[from] ProductAccountError),
    #[error("{0}")]
    StatementAllowance(#[from] StatementAllowanceError),
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
    /// Allocation helper is unavailable for this target.
    #[cfg(target_arch = "wasm32")]
    #[error("signing host: {resource} allowance allocation is native-only")]
    NativeOnly {
        /// Resource name.
        resource: &'static str,
    },
    /// System time cannot be converted into a UNIX timestamp.
    #[cfg(not(target_arch = "wasm32"))]
    #[error("system clock before UNIX epoch")]
    SystemClockBeforeUnixEpoch,
    #[error("signing account is not a personhood ring member; cannot grant {resource} allowance")]
    MissingPersonhoodMembership {
        /// Resource name.
        resource: &'static str,
    },
}

impl AllowanceAllocationError {
    pub(super) fn into_authority_error(self) -> AuthorityError {
        match self {
            Self::Authority(err) => err,
            Self::StatementAllowance(StatementAllowanceError::SessionInvalidated) => {
                AuthorityError::Disconnected
            }
            other => AuthorityError::Unavailable {
                reason: other.to_string(),
            },
        }
    }
}

/// Answer `deeplink` and serve the resulting SSO session until it ends.
#[instrument(skip_all, fields(runtime.method = "sso_responder.respond_to_pairing"))]
pub(crate) async fn respond_to_pairing(
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
pub(crate) async fn establish_pairing(
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
    let handshake = encrypt_v2_handshake_response(peer.encryption_public_key, &success)?;
    let topic = bootstrap_topic(peer.statement_account_id, peer.encryption_public_key);
    let statement = build_signed_statement(
        &session,
        topic,
        topic,
        handshake.encode(),
        fresh_statement_expiry(),
    )?;
    services
        .statement_store
        .submit(statement, "sso-responder handshake")
        .await
        .map_err(|error| error.to_string())?;
    debug!("answered pairing handshake");

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
pub(crate) async fn resume_pairing(
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
pub(crate) async fn disconnect_paired_host(
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
    let mut subscription =
        statement_store_rpc::subscribe_match_all(&rpc_client, &[session.session_id_peer])
            .await
            .map_err(|err| format!("sso-responder subscribe failed: {err}"))?;
    let mut decode_failure_request_ids = DecodeFailureRequestIds::new();

    while let Some(item) = subscription.next().await {
        let value = item.map_err(|err| format!("sso-responder subscription failed: {err}"))?;
        let page = parse_new_statements_result("sso-responder".to_string(), &value)
            .map_err(|err| err.to_string())?;
        for statement in page.statements {
            let incoming = match decode_incoming_sso_request(&session, &statement) {
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
                    // Ack a decodable envelope whose messages did not decode
                    // so the peer fails fast instead of waiting out its
                    // response deadline.
                    if let Some(request_id) = error.request_id
                        && decode_failure_request_ids.insert(request_id.clone())
                    {
                        let ack = build_signed_session_response_statement(
                            &session,
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
            for message in &incoming.messages {
                let cli_summary = format!(
                    "Incoming SSO request · {}\nstatement_request_id={}\nremote_message_id={}",
                    message.name(),
                    incoming.request_id,
                    message.message_id
                );
                tracing::event!(
                    target: "truapi_server::sso_transcript",
                    tracing::Level::DEBUG,
                    cli_summary = cli_summary.as_str(),
                    cli_event = "request_received",
                    request = message.name(),
                    statement_request_id = %incoming.request_id,
                    remote_message_id = %message.message_id,
                );
            }
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
                || serve_request(&services, &service, &session, incoming),
            )
            .await?;
            let exit = match execution {
                ReplayExecution::Duplicate => {
                    acknowledge_request(&services, &session, &request_id).await?;
                    duplicate_exit
                }
                ReplayExecution::Executed(exit) => exit,
            };
            if let Some(exit) = exit {
                return Ok(exit);
            }
        }
    }
    Ok(ResponderExit::SubscriptionEnded)
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
        let (response, outcome) = match service.dispatch(service.current_session(), message).await {
            Dispatch::Response(answer) => (answer.message, answer.outcome),
            Dispatch::Disconnected => {
                debug!("pairing host disconnected the SSO session");
                return Ok(Some(ResponderExit::PeerDisconnected));
            }
            Dispatch::NotARequest(name) => {
                warn!(name, "peer sent a response variant as a request");
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
                    target: "truapi_server::sso_transcript",
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
                    target: "truapi_server::sso_transcript",
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

pub(super) async fn allocate_statement_store_allowance(
    services: &RuntimeServices,
    signing_host: &SigningHost,
    session: &AuthoritySession,
    product_id: &str,
    policy: OnExistingAllowancePolicy,
) -> Result<Vec<u8>, AllowanceAllocationError> {
    signing_host.require_current_session(session)?;
    let entropy = signing_host.root_entropy()?;
    let allowance =
        derive_sr25519_hard_path(&entropy, &["allowance", "statement-store", product_id])?;
    register_statement_store_target(
        services,
        signing_host,
        session,
        product_id,
        allowance.public.to_bytes(),
        policy,
    )
    .await?;
    Ok(allowance.secret.to_bytes().to_vec())
}

pub(super) async fn allocate_product_statement_store_allowance(
    services: &RuntimeServices,
    signing_host: &SigningHost,
    session: &AuthoritySession,
    product_id: &str,
    derivation_index: &v01::DerivationIndex,
    policy: OnExistingAllowancePolicy,
) -> Result<(), AllowanceAllocationError> {
    signing_host.require_current_session(session)?;
    let target = signing_host
        .product_keypair(&v01::ProductAccountId {
            dot_ns_identifier: product_id.to_string(),
            derivation_index: derivation_index.clone(),
        })?
        .public
        .to_bytes();
    register_statement_store_target(services, signing_host, session, product_id, target, policy)
        .await
}

async fn register_statement_store_target(
    services: &RuntimeServices,
    signing_host: &SigningHost,
    session: &AuthoritySession,
    product_id: &str,
    target: [u8; 32],
    policy: OnExistingAllowancePolicy,
) -> Result<(), AllowanceAllocationError> {
    use crate::runtime::statement_allowance::{
        self, PooledRegistrationParams, allocated_in, find_including_rings,
        register_statement_account_pooled, scan_collections,
    };

    signing_host.require_current_session(session)?;
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
    // would choose the same one.
    let _registration = signing_host.renewal.registration_lock().lock().await;
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
    signing_host.require_current_session(session)?;
    if let Some((collection, seq)) = allocated_in(&scans) {
        debug!(
            %product_id,
            period,
            seq,
            %collection,
            "statement-store allowance already allocated"
        );
    } else {
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
    }
    signing_host.require_current_session(session)?;
    Ok(())
}

pub(super) async fn allocate_bulletin_allowance(
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
    let outcome = claim_long_term_storage(
        statement_allowance::LongTermStorageClaim {
            rpc: people_rpc,
            metadata: &chain.metadata,
            chain_state: &chain.state,
            entropy: membership.entropy,
            network_suffix: &network_suffix,
            target: &target,
            period,
            ring: &membership.ring,
        },
        || signing_host.require_current_session(session).is_ok(),
    )
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
#[cfg(not(target_arch = "wasm32"))]
pub(super) async fn allocate_smart_contract_allowance(
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

/// PGAS claims need chain access the wasm host does not have.
#[cfg(target_arch = "wasm32")]
pub(super) async fn allocate_smart_contract_allowance(
    _services: &RuntimeServices,
    _signing_host: &SigningHost,
    _session: &AuthoritySession,
    _product_id: &str,
    _derivation_index: v01::DerivationIndex,
    _policy: OnExistingAllowancePolicy,
) -> Result<(), AllowanceAllocationError> {
    Err(AllowanceAllocationError::NativeOnly { resource: "PGAS" })
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn current_unix_secs() -> Result<u64, AllowanceAllocationError> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|_| AllowanceAllocationError::SystemClockBeforeUnixEpoch)
}

#[cfg(target_arch = "wasm32")]
pub(super) fn current_unix_secs() -> Result<u64, AllowanceAllocationError> {
    Ok(statement_current_unix_secs())
}

#[cfg(test)]
mod tests {
    use super::super::LocalActivation;
    use super::*;
    use crate::host_logic::extrinsic::tests::split_v4;
    use crate::host_logic::product_account::derive_ring_vrf_domain_entropy;
    use crate::host_logic::sso::messages::{
        self, GetAccountAliasResponse, RemoteMessage, RingVrfError, SsoAllocatedResource,
        SsoAllocationOutcome,
    };
    use crate::host_logic::sso::wire::ResponseOutcome;

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
    use crate::runtime::authority::ProductAuthority;
    use crate::runtime::services::RuntimeServices;
    use crate::test_support::{StubPlatform, test_spawner};
    use std::sync::Arc;
    use truapi::latest as api;
    use truapi_platform::{HostInfo, Platform, PlatformInfo, SigningHostConfig};

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
        let signing_host = SigningHost::new(
            services.clone(),
            config.network_suffix,
            config.coinage_instance_id,
        );
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
    fn repeated_implicit_provisioning_reuses_existing_allowance_without_submission() {
        use futures::FutureExt;

        use crate::host_logic::product_account::derive_sr25519_hard_path;

        let product_id = "myapp.dot";
        let allowance =
            derive_sr25519_hard_path(&ENTROPY, &["allowance", "statement-store", product_id])
                .expect("allowance derivation succeeds");
        // The scan reads slot 0 first; answering it with an entry naming the
        // allowance account is the "already allocated" case.
        let slot_entry = (allowance.public.to_bytes(), 0u32, 0u64).encode();

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
                // The network suffix, read once before the scan.
                (
                    "state_getStorage",
                    format!(r#""0x{}""#, hex::encode(b"paseo".to_vec().encode())),
                ),
                (
                    "state_getStorage",
                    format!(r#""0x{}""#, hex::encode(&slot_entry)),
                ),
                (
                    "state_getStorage",
                    format!(r#""0x{}""#, hex::encode(b"paseo".to_vec().encode())),
                ),
                (
                    "state_getStorage",
                    format!(r#""0x{}""#, hex::encode(&slot_entry)),
                ),
                (
                    "state_getRuntimeVersion",
                    r#"{"specVersion":1000000,"transactionVersion":1}"#.to_string(),
                ),
                (
                    "chain_getBlockHash",
                    format!(r#""0x{}""#, hex::encode([0u8; 32])),
                ),
                (
                    "RuntimeViewFunction_execute_view_function",
                    format!(
                        r#""0x{}""#,
                        hex::encode(Ok::<Vec<u8>, ()>(20u32.encode()).encode()),
                    ),
                ),
            ],
            ..Default::default()
        });
        let (_services, signing_host) = signing_fixture(platform.clone());

        // Bounded, because the failure mode of losing the early return is a
        // wait on a chain read the stub deliberately does not answer — an
        // unbounded test would hang instead of reporting. The bound is generous
        // because it is catching a hang, not asserting latency.
        futures::executor::block_on(async {
            let session = signing_host.current_session().unwrap();
            let cx = truapi::CallContext::default();
            for _ in 0..2 {
                let allocation = signing_host.statement_store_allowance_key(
                    &cx,
                    &session,
                    product_id.to_string(),
                );
                futures::pin_mut!(allocation);
                let response = futures::select! {
                    result = allocation.fuse() => result,
                    _ = futures_timer::Delay::new(std::time::Duration::from_secs(30)).fuse() => {
                        panic!("allocation blocked on a chain read it should not have made")
                    }
                }
                .expect("existing allowance succeeds");
                assert_eq!(response.public_key, allowance.public.to_bytes());
            }
        });
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
        let Dispatch::Response(answer) =
            futures::executor::block_on(service.dispatch(service.current_session(), message))
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
                messages::Response {
                    responding_to: "m-1".to_string(),
                    payload: Ok([7; 32]),
                }
            )),
            Dispatch::NotARequest("ProductSubtreeResponse"),
        );
        let Dispatch::Response(answer) = dispatch(v1::RemoteMessage::ProductSubtreeRequest(
            messages::ProductSubtreeRequest {
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
    fn encrypted_chat_accepts_narrow_initialize_and_rejects_retired_operations() {
        use crate::host_logic::sso::messages::{
            ProductRequest, SsoProductDeviceChatOperation, SsoSessionStatement,
            decode_sso_session_statement,
        };
        use crate::host_logic::sso::wire::SsoRequest;
        use crate::test_support::sso_host_and_responder_sessions;
        use truapi::versioned::account::{
            HostProductDeviceChatError, HostProductDeviceChatResponse,
        };

        let platform = Arc::new(StubPlatform {
            chat_authority_confirmed: true,
            remote_permission_denied: true,
            ..Default::default()
        });
        let (_, signing_host) = signing_fixture(platform.clone());
        futures::executor::block_on(async {
            let service = SigningHostSsoService::new(signing_host);
            let (pairing, responder) = sso_host_and_responder_sessions();
            for (message_id, operation, retired) in [
                (
                    "retired-chat",
                    SsoProductDeviceChatOperation::V2(
                        truapi::v02::HostProductDeviceChatRequest::Initialize,
                    ),
                    true,
                ),
                (
                    "narrow-chat",
                    SsoProductDeviceChatOperation::V3(
                        api::HostProductDeviceChatRequest::Initialize,
                    ),
                    false,
                ),
            ] {
                let request = ProductRequest {
                    calling_product_id: "MYAPP.DOT".to_string(),
                    payload: operation,
                };
                let message = RemoteMessage::request(message_id.to_string(), request);
                let statement = messages::build_outgoing_request_statement(
                    &pairing,
                    message_id.to_string(),
                    vec![message],
                    (statement_current_unix_secs() + 60) << 32,
                )
                .unwrap();
                let incoming = messages::decode_incoming_sso_request(&responder, &statement)
                    .unwrap()
                    .unwrap();
                let Dispatch::Response(answer) = service
                    .dispatch(
                        service.current_session(),
                        incoming.messages.into_iter().next().unwrap(),
                    )
                    .await
                else {
                    panic!("expected Chat response");
                };
                let response_statement = messages::build_outgoing_request_statement(
                    &responder,
                    format!("{message_id}-response"),
                    vec![answer.message],
                    (statement_current_unix_secs() + 60) << 32,
                )
                .unwrap();
                let Some(SsoSessionStatement::RemoteMessages(mut responses)) =
                    decode_sso_session_statement(&pairing, &response_statement, message_id)
                        .unwrap()
                else {
                    panic!("expected encrypted Chat answer");
                };
                let response =
                    ProductRequest::<SsoProductDeviceChatOperation>::response_from_message(
                        responses.remove(0).unwrap(),
                    )
                    .unwrap();
                assert_eq!(response.responding_to, message_id);
                if retired {
                    assert_eq!(
                        response.payload,
                        Err(HostProductDeviceChatError::V1(
                            api::HostProductDeviceChatError::InvalidRequest,
                        ))
                    );
                    assert!(platform.chat_authority_reviews.lock().is_empty());
                } else {
                    let Ok(HostProductDeviceChatResponse::V2(state)) = response.payload else {
                        panic!("expected narrow Chat initialization");
                    };
                    assert_eq!(state.device.product_account.dot_ns_identifier, "myapp.dot");
                    assert_eq!(platform.chat_authority_reviews.lock().len(), 1);
                }
            }
            assert!(
                platform
                    .remote_permission_requests
                    .lock()
                    .unwrap()
                    .is_empty()
            );
            assert!(
                platform
                    .main_purse_chat_payment_reviews
                    .lock()
                    .unwrap()
                    .is_empty()
            );
        });
    }

    #[test]
    fn encrypted_payment_top_up_reaches_wallet_validation_without_chat_grants() {
        use crate::host_logic::sso::messages::{
            PaymentTopUpRequest, SsoSessionStatement, decode_sso_session_statement,
        };
        use crate::host_logic::sso::wire::SsoRequest;
        use crate::test_support::sso_host_and_responder_sessions;
        use truapi::versioned::payment::HostPaymentTopUpRequest;

        let platform = Arc::new(StubPlatform {
            chain_connect_error: Some("offline"),
            ..Default::default()
        });
        let (_, signing_host) = signing_fixture(platform.clone());
        futures::executor::block_on(async {
            let service = SigningHostSsoService::new(signing_host.clone());
            let (pairing, responder) = sso_host_and_responder_sessions();
            let request = PaymentTopUpRequest {
                calling_product_id: "MYAPP.DOT".to_string(),
                payload: HostPaymentTopUpRequest::V1(truapi::v01::HostPaymentTopUpRequest {
                    into: None,
                    amount: 1,
                    source: truapi::v01::PaymentTopUpSource::PrivateKey {
                        sr25519_secret_key: [0xab; 64],
                    },
                }),
            };
            let message = RemoteMessage::request("top-up-1".to_string(), request);
            assert!(!format!("{message:?}").contains(&format!("{:?}", [0xab_u8; 64])));
            let statement = messages::build_outgoing_request_statement(
                &pairing,
                "top-up-1".to_string(),
                vec![message],
                (statement_current_unix_secs() + 60) << 32,
            )
            .unwrap();
            assert!(!statement.windows(64).any(|bytes| bytes == [0xab; 64]));
            let incoming = messages::decode_incoming_sso_request(&responder, &statement)
                .unwrap()
                .unwrap();
            let Dispatch::Response(answer) = service
                .dispatch(
                    service.current_session(),
                    incoming.messages.into_iter().next().unwrap(),
                )
                .await
            else {
                panic!("expected top-up response");
            };
            let response_statement = messages::build_outgoing_request_statement(
                &responder,
                "top-up-response".to_string(),
                vec![answer.message],
                (statement_current_unix_secs() + 60) << 32,
            )
            .unwrap();
            let Some(SsoSessionStatement::RemoteMessages(mut responses)) =
                decode_sso_session_statement(&pairing, &response_statement, "top-up-1").unwrap()
            else {
                panic!("expected encrypted top-up answer");
            };
            let response =
                PaymentTopUpRequest::response_from_message(responses.remove(0).unwrap()).unwrap();
            assert_eq!(response.responding_to, "top-up-1");
            assert_eq!(
                response.payload,
                Err(messages::PaymentTopUpError(
                    truapi::v01::HostPaymentTopUpError::InvalidSource
                ))
            );
            assert!(platform.chat_authority_reviews.lock().is_empty());
            assert!(
                platform
                    .main_purse_chat_payment_reviews
                    .lock()
                    .unwrap()
                    .is_empty()
            );
            assert!(platform.sign_raw_reviews.lock().unwrap().is_empty());
            assert!(platform.sign_payload_reviews.lock().unwrap().is_empty());
            assert_eq!(
                platform
                    .identity_disclosure_calls
                    .load(std::sync::atomic::Ordering::SeqCst),
                0
            );

            signing_host.disconnect().await;
            let request = RemoteMessage::request(
                "top-up-disconnected".to_string(),
                PaymentTopUpRequest {
                    calling_product_id: "myapp.dot".to_string(),
                    payload: HostPaymentTopUpRequest::V1(truapi::v01::HostPaymentTopUpRequest {
                        into: None,
                        amount: 1,
                        source: truapi::v01::PaymentTopUpSource::Coins {
                            sr25519_secret_keys: vec![[0xab; 64]],
                        },
                    }),
                },
            );
            let Dispatch::Response(answer) =
                service.dispatch(service.current_session(), request).await
            else {
                panic!("expected disconnected top-up response");
            };
            let RemoteMessageData::V1(data) = answer.message.data;
            let response = PaymentTopUpRequest::response_from_message(data).unwrap();
            assert!(matches!(
                response.payload,
                Err(messages::PaymentTopUpError(
                    truapi::v01::HostPaymentTopUpError::Unknown { .. }
                ))
            ));
        });
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
            messages::ResourceAllocationRequest {
                calling_product_id: "myapp.dot".to_string(),
                resources: vec![
                    api::AllocatableResource::AutoSigning,
                    api::AllocatableResource::BulletinAllowance,
                    api::AllocatableResource::StatementStoreAllowance,
                ],
                on_existing: OnExistingAllowancePolicy::Ignore,
            },
        );
        let Dispatch::Response(answer) =
            futures::executor::block_on(service.dispatch(service.current_session(), request))
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
            v1::RemoteMessage::GetAccountAliasRequest(messages::ProductRequest {
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
            v1::RemoteMessage::ResourceAllocationRequest(messages::ResourceAllocationRequest {
                calling_product_id: "myapp.dot".to_string(),
                resources: vec![api::AllocatableResource::StatementStoreAllowance],
                on_existing: messages::OnExistingAllowancePolicy::Ignore,
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
            v1::RemoteMessage::ResourceAllocationRequest(messages::ResourceAllocationRequest {
                calling_product_id: "myapp.dot".to_string(),
                resources: vec![api::AllocatableResource::AutoSigning],
                on_existing: messages::OnExistingAllowancePolicy::Ignore,
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
            messages::ResourceAllocationRequest {
                calling_product_id: "myapp.dot".to_string(),
                resources: vec![api::AllocatableResource::AutoSigning],
                on_existing: OnExistingAllowancePolicy::Ignore,
            },
        );

        futures::executor::block_on(async {
            let answer = service.dispatch(service.current_session(), message);
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
                messages::CreateTransactionWithLegacyAccountRequest {
                    payload: messages::CreateTransactionLegacyPayload::V1(payload),
                },
            ),
        );

        let v1::RemoteMessage::CreateTransactionResponse(response) = response else {
            panic!("expected create transaction response");
        };
        let transaction = response.payload.expect("identity transaction succeeds");
        let (account, signature, tail) = split_v4(&transaction);
        assert_eq!(account, identity.public.to_bytes());
        assert_eq!(tail, vec![1, 0x00, 0x00]);
        let signature = schnorrkel::Signature::from_bytes(&signature).unwrap();
        assert!(
            identity
                .public
                .verify_simple(b"substrate", &[0x00, 0x00, 1, 2, 3], &signature)
                .is_ok()
        );
    }

    #[test]
    fn product_subtree_request_is_consent_free_and_hard_derived() {
        let (_, signing_host) = signing_fixture(Arc::new(StubPlatform::default()));
        let response = answer(
            &signing_host,
            "subtree-1",
            v1::RemoteMessage::ProductSubtreeRequest(messages::ProductSubtreeRequest {
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
