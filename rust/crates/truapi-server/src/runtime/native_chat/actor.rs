// SPDX-License-Identifier: AGPL-3.0-only
//! Native Chat custody and durable transport. Wire algorithms derive from the
//! AGPL useragent-chat-v2 implementation; no spendable plaintext crosses TrUAPI.

mod files;
mod history;
mod receive;
#[cfg(test)]
mod tests;

use parity_scale_codec::{Decode, Encode};
use schnorrkel::Keypair;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use truapi::latest::*;
use truapi_chat_v2 as wire;
use zeroize::{Zeroize, Zeroizing};

use super::{
    NativeChatContext, NativeChatRegistry, identity,
    payments::{PaymentIntent, PaymentTransport},
    store::ChatStateStore,
};
use crate::host_logic::statement_store::{
    current_unix_secs, decode_signed_statement, sign_statement_fields, signed_statement_to_scale,
    statement_fields_from_v01,
};
use crate::host_logic::{product_account::*, sso::pairing::derive_identity_chat_private_key};
use crate::runtime::{
    chat_device::{HostChatDevice, PeerDevice, validate_guest_messages},
    chat_identity::*,
};

type Error = HostProductDeviceChatError;
const MAX_PEERS: usize = 256;
const MAX_OUTBOX: usize = 256;
const MAX_RECEIPTS: usize = 4096;
const MAX_HISTORY_BATCHES: usize = 256;
const LIFETIME: u64 = 2 * 86_400;
const CLOCK_SKEW: u64 = 300;

#[derive(Clone, Encode, Decode)]
struct Secret32([u8; 32]);
impl Drop for Secret32 {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[derive(Clone, Encode, Decode)]
struct DeviceRecord {
    account: [u8; 32],
    key: Option<[u8; 32]>,
    active: bool,
    timestamp: u64,
    message_id: String,
}

#[derive(Clone, Encode, Decode)]
struct Peer {
    identity: [u8; 32],
    root_key: [u8; 32],
    username: Option<String>,
    devices: Vec<DeviceRecord>,
    invitation: Option<String>,
    invitation_text: Option<String>,
    invitation_timestamp: Option<u64>,
    established: bool,
    revocation_request: Option<String>,
    revocation_acked: bool,
    revocation_acks: Vec<[u8; 32]>,
    revision: u64,
}

impl Peer {
    fn active_devices(&self) -> Vec<PeerDevice> {
        self.devices
            .iter()
            .filter_map(|device| {
                if !device.active {
                    return None;
                }
                Some(PeerDevice {
                    account_id: device.account,
                    public_key: device.key?,
                })
            })
            .collect()
    }
    fn ready(&self) -> bool {
        self.established && self.revocation_acked && self.devices.iter().any(|device| device.active)
    }
    fn payment_devices(&self) -> impl Iterator<Item = PeerDevice> + '_ {
        self.devices.iter().filter_map(|device| {
            if !device.active || !self.revocation_acks.contains(&device.account) {
                return None;
            }
            Some(PeerDevice {
                account_id: device.account,
                public_key: device.key?,
            })
        })
    }
    fn ready_for_payments(&self) -> bool {
        self.established && self.payment_devices().next().is_some()
    }
}

#[derive(Clone, Encode, Decode)]
struct Invitation {
    id: [u8; 32],
    peer: [u8; 32],
    root_key: [u8; 32],
    username: Option<String>,
    device_account: [u8; 32],
    device_key: [u8; 32],
    message_id: String,
    timestamp: u64,
    text: String,
}

#[derive(Clone, PartialEq, Eq, Encode, Decode)]
enum OutgoingKind {
    Invitation,
    Acceptance,
    Revocation,
    Ordinary,
    Payment([u8; 32]),
    Acknowledgment,
    Rich([u8; 32]),
}

#[derive(Clone, Encode, Decode)]
struct Outgoing {
    peer: [u8; 32],
    request_id: String,
    digest: [u8; 32],
    kind: OutgoingKind,
    roster_revision: u64,
    statement: SignedStatement,
    last_attempt: u64,
}

#[derive(Clone, Encode, Decode)]
struct Receipt {
    peer: [u8; 32],
    request_id: String,
    digest: [u8; 32],
    timestamp: u64,
}

#[derive(Clone, Encode, Decode)]
struct SentReceipt {
    peer: [u8; 32],
    request_id: String,
    wire_request_id: String,
    digest: [u8; 32],
}

#[derive(Clone, Encode, Decode)]
struct State {
    secret: Secret32,
    index: [u8; 32],
    peers: Vec<Peer>,
    invitations: Vec<Invitation>,
    outbox: Vec<Outgoing>,
    received: Vec<Receipt>,
    sent: Vec<SentReceipt>,
    accepted_payments: Vec<[u8; 32]>,
    payment_acknowledgments: Vec<[u8; 32]>,
    messages: Vec<HostNativeChatMessages>,
    acknowledgments: Vec<HostNativeChatAcknowledgment>,
    last_expiry: u64,
    history_imports: Vec<history::HistoryImport>,
    files: Vec<files::FileRecord>,
    rich_messages: Vec<files::RichRecord>,
}

impl State {
    fn initial() -> Result<Self, Error> {
        Ok(Self {
            secret: Secret32(random_bytes()?),
            index: random_bytes()?,
            peers: Vec::new(),
            invitations: Vec::new(),
            outbox: Vec::new(),
            received: Vec::new(),
            sent: Vec::new(),
            accepted_payments: Vec::new(),
            payment_acknowledgments: Vec::new(),
            messages: Vec::new(),
            acknowledgments: Vec::new(),
            last_expiry: 0,
            history_imports: Vec::new(),
            files: Vec::new(),
            rich_messages: Vec::new(),
        })
    }
    fn peer(&self, identity: &[u8; 32]) -> Result<&Peer, Error> {
        self.peers
            .iter()
            .find(|peer| &peer.identity == identity)
            .ok_or(Error::PeerNotReady)
    }
    fn peer_mut(&mut self, identity: &[u8; 32]) -> Result<&mut Peer, Error> {
        self.peers
            .iter_mut()
            .find(|peer| &peer.identity == identity)
            .ok_or(Error::PeerNotReady)
    }
    fn expire_receipts(&mut self, now: u64) {
        self.received
            .retain(|receipt| now <= receipt.timestamp.saturating_add(LIFETIME));
        self.invitations
            .retain(|invite| fresh(invite.timestamp, now));
    }
    fn queue(&mut self, outgoing: Outgoing) -> Result<(), Error> {
        if let Some(existing) = self.outbox.iter_mut().find(|entry| {
            entry.peer == outgoing.peer
                && entry.request_id == outgoing.request_id
                && entry.kind == outgoing.kind
        }) {
            if existing.digest != outgoing.digest {
                return Err(Error::OperationConflict);
            }
            if existing.kind == OutgoingKind::Acknowledgment
                && (existing.statement.topics != outgoing.statement.topics
                    || existing.statement.channel != outgoing.statement.channel)
            {
                // Authenticated replay repairs queued ACKs from the old reversed
                // route without replacing any message or payment commitment.
                *existing = outgoing;
            }
            return Ok(());
        }
        if self.outbox.len() >= MAX_OUTBOX {
            return Err(Error::StorageUnavailable);
        }
        self.outbox.push(outgoing);
        Ok(())
    }
    fn record_messages(&mut self, messages: HostNativeChatMessages) {
        if messages.messages.is_empty() {
            return;
        }
        if self.messages.len() == MAX_HISTORY_BATCHES {
            self.messages.remove(0);
        }
        self.messages.push(messages);
    }
}

pub(super) struct NativeChatActor {
    product: String,
    public: HostNativeChatDevice,
    legacy_account: [u8; 32],
    root_secret: Zeroizing<[u8; 32]>,
    signer: Keypair,
    device: HostChatDevice,
    store: Arc<ChatStateStore<State>>,
    delivering: AtomicBool,
    receiving: futures::lock::Mutex<()>,
    delivery_gate: futures::lock::Mutex<()>,
    history_ack_gate: futures::lock::Mutex<()>,
    file_selection_gate: futures::lock::Mutex<()>,
    file_transfer_gate: futures::lock::Mutex<()>,
    file_export_gate: futures::lock::Mutex<()>,
    file_cursor: AtomicUsize,
}

impl NativeChatActor {
    pub(super) async fn open(
        context: &NativeChatContext,
        product: &str,
    ) -> Result<Arc<Self>, Error> {
        let store = ChatStateStore::open(context, product, State::initial).await?;
        let (index, secret) = store
            .read(|state| (state.index, Zeroizing::new(state.secret.0)))
            .await?;
        let root = derive_root_keypair_from_entropy(&context.entropy)
            .map_err(|_| Error::InvalidRequest)?;
        let signer =
            derive_product_keypair(&root, product, index).map_err(|_| Error::InvalidRequest)?;
        let legacy_account = derive_product_keypair(&root, product, index_bytes(0))
            .map_err(|_| Error::InvalidRequest)?
            .public
            .to_bytes();
        let identity_account_id =
            derive_identity_keypair(&context.entropy, &context.network_suffix)
                .map_err(|_| Error::InvalidRequest)?
                .public
                .to_bytes();
        let root_secret = Zeroizing::new(derive_identity_chat_private_key(&context.entropy));
        let device = HostChatDevice::from_secret(signer.public.to_bytes(), *secret);
        let public = HostNativeChatDevice {
            identity_account_id,
            identity_chat_public_key: wire::x25519_public_key(&root_secret),
            product_account: ProductAccountId {
                dot_ns_identifier: product.to_owned(),
                derivation_index: DerivationIndex::Raw(index),
            },
            account_id: signer.public.to_bytes(),
            chat_public_key: device.public_key(),
        };
        let actor = Arc::new(Self {
            product: product.to_owned(),
            public,
            legacy_account,
            root_secret,
            signer,
            device,
            store,
            delivering: AtomicBool::new(false),
            receiving: futures::lock::Mutex::new(()),
            delivery_gate: futures::lock::Mutex::new(()),
            history_ack_gate: futures::lock::Mutex::new(()),
            file_selection_gate: futures::lock::Mutex::new(()),
            file_transfer_gate: futures::lock::Mutex::new(()),
            file_export_gate: futures::lock::Mutex::new(()),
            file_cursor: AtomicUsize::new(0),
        });
        actor
            .store
            .read(|state| actor.validate_state(state))
            .await??;
        Ok(actor)
    }

    fn validate_state(&self, state: &State) -> Result<(), Error> {
        if state.peers.len() > MAX_PEERS
            || state.invitations.len() > 16
            || state.outbox.len() > MAX_OUTBOX
            || state.received.len() > MAX_RECEIPTS
            || state.sent.len() > MAX_RECEIPTS
            || state.accepted_payments.len() > MAX_RECEIPTS
            || state.messages.len() > MAX_HISTORY_BATCHES
            || state.payment_acknowledgments.len() > MAX_RECEIPTS
            || state.acknowledgments.len() > MAX_HISTORY_BATCHES
        {
            return Err(Error::StorageUnavailable);
        }
        history::validate_imports(&state.history_imports)?;
        files::validate(state)?;
        let mut identities = std::collections::HashSet::new();
        for peer in &state.peers {
            if peer.identity == self.public.identity_account_id
                || !identities.insert(peer.identity)
                || peer.devices.len() > 64
                || peer.active_devices().len() > 16
            {
                return Err(Error::StorageUnavailable);
            }
            chat_shared_secret(&self.root_secret, &peer.root_key)
                .map_err(|_| Error::StorageUnavailable)?;
            let mut accounts = std::collections::HashSet::new();
            for device in &peer.devices {
                if !accounts.insert(device.account) || (device.active && device.key.is_none()) {
                    return Err(Error::StorageUnavailable);
                }
                if let Some(key) = device.key {
                    self.device
                        .identity_shared_secret(&key)
                        .map_err(|_| Error::StorageUnavailable)?;
                }
            }
        }
        Ok(())
    }

    fn peer_incoming_topics(&self, peer: &Peer) -> Result<Vec<[u8; 32]>, Error> {
        let shared = chat_shared_secret(&self.root_secret, &peer.root_key)
            .map_err(|_| Error::InvalidStatement)?;
        // Native peers publish requests and responses on their own outgoing
        // session. Never subscribe to our outgoing route to discover peer ACKs.
        let mut topics = vec![chat_identity_session_id(
            &shared,
            &peer.identity,
            &self.public.identity_account_id,
        )];
        for device in peer.active_devices() {
            let incoming = chat_shared_secret(&self.root_secret, &device.public_key)
                .map_err(|_| Error::InvalidStatement)?;
            topics.push(chat_identity_session_id(
                &incoming,
                &device.account_id,
                &self.public.identity_account_id,
            ));
        }
        topics.sort_unstable();
        topics.dedup();
        Ok(topics)
    }

    pub(super) async fn incoming_topics(&self) -> Result<Vec<[u8; 32]>, Error> {
        self.store
            .read(|state| {
                let mut topics = vec![wire::chat_request_full_topic(
                    &self.public.identity_account_id,
                )];
                for peer in &state.peers {
                    topics.extend(self.peer_incoming_topics(peer)?);
                }
                topics.sort_unstable();
                topics.dedup();
                Ok(topics)
            })
            .await?
    }

    pub(super) async fn public_view(
        &self,
        context: &NativeChatContext,
        payments: Vec<HostNativeChatPayment>,
    ) -> Result<HostProductDeviceChatResponse, Error> {
        context.require_current()?;
        self.store
            .read(|state| {
                let peers = state
                    .peers
                    .iter()
                    .map(|peer| {
                        let topics = self.peer_incoming_topics(peer)?;
                        let devices = peer.active_devices();
                        Ok(HostNativeChatPeer {
                            identity_account_id: peer.identity,
                            username: peer.username.clone(),
                            devices: devices
                                .into_iter()
                                .map(|device| HostNativeChatPeerDevice {
                                    account_id: device.account_id,
                                    chat_public_key: device.public_key,
                                })
                                .collect(),
                            incoming_channels: topics,
                            ready_for_payments: peer.ready_for_payments(),
                        })
                    })
                    .collect::<Result<Vec<_>, Error>>()?;
                Ok(HostProductDeviceChatResponse {
                    device: self.public.clone(),
                    peers,
                    invitations: state
                        .invitations
                        .iter()
                        .filter(|invite| fresh(invite.timestamp, current_unix_secs()))
                        .map(|invite| HostNativeChatInvitation {
                            invitation_id: invite.id,
                            peer_identity: invite.peer,
                            username: invite.username.clone(),
                            timestamp: invite.timestamp,
                            text: invite.text.clone(),
                        })
                        .collect(),
                    messages: state.messages.clone(),
                    acknowledgments: state.acknowledgments.clone(),
                    payments,
                    rich_messages: files::public_views(state)?,
                })
            })
            .await?
    }

    pub(super) async fn invite(
        self: &Arc<Self>,
        context: &NativeChatContext,
        username: String,
        text: String,
    ) -> Result<(), Error> {
        if text.len() > 8192 {
            return Err(Error::InvalidRequest);
        }
        let resolved = identity::resolve_username(context, &username).await?;
        context.require_current()?;
        if resolved.identity_account_id == self.public.identity_account_id {
            return Err(Error::InvalidRequest);
        }
        let actor = self.clone();
        let valid = context.session_valid.clone();
        self.store
            .update(move |state| {
                if !valid() {
                    return Err(Error::NotConnected);
                }
                if let Some(peer) = state
                    .peers
                    .iter()
                    .find(|peer| peer.identity == resolved.identity_account_id)
                {
                    if peer.root_key != resolved.chat_public_key {
                        return Err(Error::InvalidStatement);
                    }
                    if peer.invitation.is_some() || peer.established {
                        return if peer.invitation_text.as_deref() == Some(&text) {
                            Ok(())
                        } else {
                            Err(Error::OperationConflict)
                        };
                    }
                } else {
                    if state.peers.len() >= MAX_PEERS {
                        return Err(Error::StorageUnavailable);
                    }
                    state.peers.push(Peer {
                        identity: resolved.identity_account_id,
                        root_key: resolved.chat_public_key,
                        username: resolved.username,
                        devices: Vec::new(),
                        invitation: None,
                        invitation_text: None,
                        invitation_timestamp: None,
                        established: false,
                        revocation_request: None,
                        revocation_acked: false,
                        revocation_acks: Vec::new(),
                        revision: 0,
                    });
                }
                let peer = state.peer(&resolved.identity_account_id)?.clone();
                let shared = chat_shared_secret(&actor.root_secret, &peer.root_key)
                    .map_err(|_| Error::InvalidStatement)?;
                let request_id = random_id()?;
                let now = current_unix_secs();
                state.peer_mut(&peer.identity)?.invitation_text = Some(text.clone());
                if !text.is_empty() {
                    state.record_messages(HostNativeChatMessages {
                        peer_identity: peer.identity,
                        incoming: false,
                        request_id: request_id.clone(),
                        messages: vec![
                            wire::encode_rich_text_message(
                                &request_id,
                                now.saturating_mul(1000),
                                Some(&text),
                                None,
                            )
                            .map_err(|_| Error::InvalidRequest)?,
                        ],
                    });
                }
                let message = wire::V2ChatRequestMessageV2 {
                    message_id: request_id.clone(),
                    timestamp: now.saturating_mul(1000),
                    content: wire::V2ChatRequestContentV2 {
                        identity_proof: wire::V2ChatRequestIdentityProof {
                            identity_account_id: actor.public.identity_account_id,
                            proof: chat_device_identity_proof(
                                &shared,
                                &actor.public.identity_account_id,
                                &actor.public.account_id,
                            ),
                        },
                        device_enc_pub_key: actor.public.chat_public_key,
                        push_token: None,
                        welcome_text: (!text.is_empty()).then_some(text),
                    },
                };
                let payload = wire::encode_chat_request_v2_proof_payload(&message, &peer.identity)
                    .map_err(|_| Error::InvalidRequest)?;
                let signature = actor
                    .signer
                    .secret
                    .sign_simple(SR25519_SIGNING_CONTEXT, &payload, &actor.signer.public)
                    .to_bytes();
                let request = wire::V2ChatRequestV2 {
                    message,
                    proof: wire::V2ChatRequestProof {
                        signature: signature.to_vec(),
                        signer: actor.public.account_id.to_vec(),
                    },
                };
                let ephemeral = Zeroizing::new(random_bytes()?);
                let data = wire::seal_chat_request_v2_with_nonce(
                    &ephemeral,
                    &peer.root_key,
                    &request,
                    random_bytes()?,
                )
                .map_err(|_| Error::InvalidRequest)?;
                let channel = chat_request_channel_id(
                    &shared,
                    &actor.public.identity_account_id,
                    &peer.identity,
                );
                let day = wire::chat_request_day_from_unix(now).ok_or(Error::InvalidRequest)?;
                let statement = actor.sign(
                    state,
                    channel,
                    vec![
                        wire::chat_request_full_topic(&peer.identity),
                        wire::chat_request_day_topic(&peer.identity, day),
                    ],
                    data,
                )?;
                state.peer_mut(&peer.identity)?.invitation = Some(request_id.clone());
                state.peer_mut(&peer.identity)?.invitation_timestamp =
                    Some(now.saturating_mul(1000));
                state.queue(Outgoing {
                    peer: peer.identity,
                    request_id,
                    digest: hash(&payload),
                    kind: OutgoingKind::Invitation,
                    roster_revision: peer.revision,
                    statement,
                    last_attempt: 0,
                })
            })
            .await?;
        self.start_delivery(context);
        self.flush(context).await
    }

    pub(super) async fn reject(
        &self,
        context: &NativeChatContext,
        invitation_id: [u8; 32],
    ) -> Result<(), Error> {
        let valid = context.session_valid.clone();
        self.store
            .update(move |state| {
                if !valid() {
                    return Err(Error::NotConnected);
                }
                let position = state
                    .invitations
                    .iter()
                    .position(|invite| invite.id == invitation_id)
                    .ok_or(Error::InvalidRequest)?;
                state.invitations.remove(position);
                Ok(())
            })
            .await
    }

    pub(super) async fn send(
        self: &Arc<Self>,
        context: &NativeChatContext,
        peer_identity: [u8; 32],
        request_id: String,
        messages: Vec<Vec<u8>>,
    ) -> Result<(), Error> {
        valid_id(&request_id)?;
        validate_guest_messages(&messages).map_err(|_| Error::InvalidRequest)?;
        let digest = hash(&messages.encode());
        let wire_request_id = format!(
            "msg-{}",
            hex::encode(hash(
                &(
                    b"truapi/native-chat/ordinary/v1".as_slice(),
                    context.genesis_hash,
                    self.public.account_id,
                    &self.product,
                    peer_identity,
                    &request_id,
                )
                    .encode()
            ))
        );
        let actor = self.clone();
        let valid = context.session_valid.clone();
        self.store
            .update(move |state| {
                if !valid() {
                    return Err(Error::NotConnected);
                }
                if let Some(old) = state
                    .sent
                    .iter()
                    .find(|old| old.peer == peer_identity && old.request_id == request_id)
                {
                    return if old.digest == digest {
                        Ok(())
                    } else {
                        Err(Error::OperationConflict)
                    };
                }
                if state.sent.len() >= MAX_RECEIPTS {
                    return Err(Error::StorageUnavailable);
                }
                let peer = state.peer(&peer_identity)?.clone();
                let devices = peer.active_devices();
                if !peer.established || devices.is_empty() {
                    return Err(Error::PeerNotReady);
                }
                let statement =
                    actor.multi_statement(state, &peer, &devices, &wire_request_id, &messages)?;
                state.sent.push(SentReceipt {
                    peer: peer_identity,
                    request_id,
                    wire_request_id: wire_request_id.clone(),
                    digest,
                });
                state.queue(Outgoing {
                    peer: peer_identity,
                    request_id: wire_request_id,
                    digest,
                    kind: OutgoingKind::Ordinary,
                    roster_revision: peer.revision,
                    statement,
                    last_attempt: 0,
                })
            })
            .await?;
        self.start_delivery(context);
        self.flush(context).await
    }

    pub(super) async fn payment(
        self: &Arc<Self>,
        context: &NativeChatContext,
        peer_identity: [u8; 32],
        request_id: String,
        amount_cents: u64,
    ) -> Result<(PaymentIntent, Arc<dyn PaymentTransport>), Error> {
        valid_id(&request_id)?;
        if amount_cents == 0 {
            return Err(Error::InvalidRequest);
        }
        let peer = self
            .store
            .read(|state| state.peer(&peer_identity).cloned())
            .await??;
        if !peer.ready_for_payments() {
            return Err(Error::PeerNotReady);
        }
        // Resolve the authoritative recipient again for the trusted review.
        let resolved = identity::resolve_account(context, peer_identity).await?;
        if resolved.chat_public_key != peer.root_key {
            return Err(Error::InvalidStatement);
        }
        let intent = PaymentIntent {
            product_id: self.product.clone(),
            peer_identity,
            recipient_username: resolved.username,
            request_id,
            amount_cents,
        };
        let transport = Arc::new(ChatPaymentTransport {
            actor: self.clone(),
            context: context.clone(),
            peer_identity,
        });
        Ok((intent, transport))
    }

    fn multi_statement(
        &self,
        state: &mut State,
        peer: &Peer,
        devices: &[PeerDevice],
        request_id: &str,
        messages: &[Vec<u8>],
    ) -> Result<SignedStatement, Error> {
        let plaintext = Zeroizing::new(
            wire::encode_transport_request_plaintext(request_id, messages)
                .map_err(|_| Error::InvalidRequest)?,
        );
        let inner = Zeroizing::new(
            self.device
                .seal_multi_device(devices, &plaintext)
                .map_err(|_| Error::InvalidStatement)?,
        );
        let shared = self
            .device
            .identity_shared_secret(&peer.root_key)
            .map_err(|_| Error::InvalidStatement)?;
        let session = chat_identity_session_id(&shared, &self.public.account_id, &peer.identity);
        let channel =
            wire::chat_identity_request_topic(&session).map_err(|_| Error::InvalidStatement)?;
        let encrypted = native_root_seal(&shared, &inner).map_err(|_| Error::InvalidStatement)?;
        self.sign(state, channel, vec![session], encrypted)
    }

    fn sign(
        &self,
        state: &mut State,
        channel: [u8; 32],
        topics: Vec<[u8; 32]>,
        data: Vec<u8>,
    ) -> Result<SignedStatement, Error> {
        let candidate = current_unix_secs()
            .checked_add(LIFETIME)
            .and_then(|value| value.checked_mul(1u64 << 32))
            .ok_or(Error::InvalidRequest)?;
        let expiry = candidate.max(
            state
                .last_expiry
                .checked_add(1)
                .ok_or(Error::InvalidRequest)?,
        );
        state.last_expiry = expiry;
        let fields = statement_fields_from_v01(Statement {
            proof: None,
            decryption_key: None,
            expiry: Some(expiry),
            channel: Some(channel),
            topics,
            data: Some(data),
        })
        .map_err(|_| Error::InvalidRequest)?;
        let secret = Zeroizing::new(self.signer.secret.to_bytes());
        let fields = sign_statement_fields(*secret, self.public.account_id, fields)
            .map_err(|_| Error::InvalidRequest)?;
        decode_signed_statement(&fields.encode()).map_err(|_| Error::InvalidRequest)
    }

    pub(super) async fn flush(&self, context: &NativeChatContext) -> Result<(), Error> {
        let _delivery = self.delivery_gate.lock().await;
        context.require_current()?;
        let queued = self.store.read(|state| state.outbox.clone()).await?;
        let mut failure = None;
        for outgoing in queued {
            match self.flush_outgoing(context, outgoing).await {
                Ok(()) => {}
                Err(error @ (Error::NetworkUnavailable | Error::AllowanceRequired)) => {
                    // One offline/full route must not starve another peer's
                    // acceptance, revocation, or acknowledgment.
                    failure.get_or_insert(error);
                }
                Err(error) => return Err(error),
            }
        }
        failure.map_or(Ok(()), Err)
    }

    async fn refresh_statement(
        &self,
        peer: [u8; 32],
        request_id: &str,
        kind: &OutgoingKind,
        minimum_expiry: u64,
    ) -> Result<Option<SignedStatement>, Error> {
        let key = (peer, request_id.to_owned(), kind.clone());
        let signer_secret = Zeroizing::new(self.signer.secret.to_bytes());
        let signer_public = self.public.account_id;
        self.store
            .update(move |state| {
                let Some(position) = state.outbox.iter().position(|entry| {
                    entry.peer == key.0 && entry.request_id == key.1 && entry.kind == key.2
                }) else {
                    // An authenticated ACK may have retired it while the RPC
                    // was in flight. Never resurrect the acknowledged request.
                    return Ok(None);
                };
                let old = &state.outbox[position].statement;
                let expiry = current_unix_secs()
                    .checked_add(LIFETIME)
                    .and_then(|value| value.checked_mul(1u64 << 32))
                    .ok_or(Error::InvalidRequest)?
                    .max(
                        state
                            .last_expiry
                            .checked_add(1)
                            .ok_or(Error::InvalidRequest)?,
                    )
                    .max(minimum_expiry);
                // Only the signed priority changes. Keep the committed
                // ciphertext, routing, signer, request ID, and payment intact.
                let fields = statement_fields_from_v01(Statement {
                    proof: None,
                    decryption_key: None,
                    expiry: Some(expiry),
                    channel: old.channel,
                    topics: old.topics.clone(),
                    data: old.data.clone(),
                })
                .map_err(|_| Error::InvalidRequest)?;
                let signed = decode_signed_statement(
                    &sign_statement_fields(*signer_secret, signer_public, fields)
                        .map_err(|_| Error::InvalidRequest)?
                        .encode(),
                )
                .map_err(|_| Error::InvalidRequest)?;
                state.last_expiry = expiry;
                state.outbox[position].statement = signed.clone();
                Ok(Some(signed))
            })
            .await
    }

    async fn flush_outgoing(
        &self,
        context: &NativeChatContext,
        mut outgoing: Outgoing,
    ) -> Result<(), Error> {
        context.require_current()?;
        let allowed = self
            .store
            .read(|state| {
                state.peer(&outgoing.peer).is_ok_and(|peer| {
                    matches!(
                        outgoing.kind,
                        OutgoingKind::Invitation
                            | OutgoingKind::Acceptance
                            | OutgoingKind::Acknowledgment
                    ) || (peer.established
                        && peer.revision == outgoing.roster_revision
                        && !peer.active_devices().is_empty())
                })
            })
            .await?;
        let now = current_unix_secs();
        if !allowed || (outgoing.last_attempt != 0 && now < outgoing.last_attempt.saturating_add(5))
        {
            return Ok(());
        }
        if outgoing
            .statement
            .expiry
            .is_none_or(|expiry| (expiry >> 32) <= now)
        {
            let Some(statement) = self
                .refresh_statement(outgoing.peer, &outgoing.request_id, &outgoing.kind, 0)
                .await?
            else {
                return Ok(());
            };
            outgoing.statement = statement;
        }
        let rpc = context
            .services
            .statement_store
            .client("native_chat.delivery")
            .await
            .map_err(|_| Error::NetworkUnavailable)?;
        // A definite priority rejection permits one expiry-only retry. An
        // ambiguous network failure does not authorize replacing the statement.
        for attempt in 0..2 {
            context.require_current()?;
            let bytes =
                signed_statement_to_scale(outgoing.statement).map_err(|_| Error::InvalidRequest)?;
            match crate::runtime::statement_store_rpc::submit(&rpc, bytes).await {
                Ok(()) => break,
                Err(error) => {
                    if attempt == 0
                        && let Some(expiry) = error.replacement_expiry()
                    {
                        context.require_current()?;
                        let Some(statement) = self
                            .refresh_statement(
                                outgoing.peer,
                                &outgoing.request_id,
                                &outgoing.kind,
                                expiry,
                            )
                            .await?
                        else {
                            return Ok(());
                        };
                        outgoing.statement = statement;
                        continue;
                    }
                    return Err(if error.is_no_allowance() {
                        Error::AllowanceRequired
                    } else {
                        Error::NetworkUnavailable
                    });
                }
            }
        }
        self.store
            .update(move |state| {
                if outgoing.kind == OutgoingKind::Acknowledgment {
                    state.outbox.retain(|entry| {
                        !(entry.peer == outgoing.peer
                            && entry.request_id == outgoing.request_id
                            && entry.kind == OutgoingKind::Acknowledgment)
                    });
                } else if let Some(entry) = state.outbox.iter_mut().find(|entry| {
                    entry.peer == outgoing.peer
                        && entry.request_id == outgoing.request_id
                        && entry.kind == outgoing.kind
                }) {
                    entry.last_attempt = now;
                }
                Ok(())
            })
            .await
    }

    fn start_delivery(self: &Arc<Self>, context: &NativeChatContext) {
        if self
            .delivering
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let actor = self.clone();
        let context = context.clone();
        let spawner = context.services.spawner.clone();
        spawner(Box::pin(async move {
            struct Running(Arc<NativeChatActor>);
            impl Drop for Running {
                fn drop(&mut self) {
                    self.0.delivering.store(false, Ordering::Release);
                }
            }
            let _running = Running(actor.clone());
            while (context.session_valid)() {
                if super::background::require_authorized(&context, &actor.product)
                    .await
                    .is_err()
                {
                    break;
                }
                if actor
                    .store
                    .read(|state| {
                        state.outbox.is_empty()
                            && !history::has_pending(&state.history_imports)
                            && !files::has_pending(state)
                    })
                    .await
                    .unwrap_or(true)
                {
                    break;
                }
                let (_, _, files) = futures::join!(
                    actor.flush(&context),
                    actor.acknowledge_history(&context),
                    actor.drive_files(&context)
                );
                let delay = if matches!(files, Ok(true)) { 1 } else { 5000 };
                futures_timer::Delay::new(std::time::Duration::from_millis(delay)).await;
            }
        }));
    }

    pub(super) async fn reconcile(
        self: &Arc<Self>,
        context: &NativeChatContext,
        registry: &NativeChatRegistry,
    ) -> Result<(), Error> {
        context.require_current()?;
        self.store.reauthenticate().await?;
        let wallet = registry.wallet(context).await?;
        self.replay_payment_acknowledgments(context, registry)
            .await?;
        let accepted = self
            .store
            .read(|state| state.accepted_payments.clone())
            .await?;
        for payment in wallet
            .pending_handoffs(context, &self.product, &accepted)
            .await?
        {
            let transport = Arc::new(ChatPaymentTransport {
                actor: self.clone(),
                context: context.clone(),
                peer_identity: payment.peer_identity,
            });
            wallet
                .redeliver(context, &self.product, payment.operation_id, transport)
                .await?;
        }
        self.start_delivery(context);
        let (delivery, settlement, history) = futures::join!(
            self.flush(context),
            wallet.reconcile(context),
            self.acknowledge_history(context)
        );
        delivery?;
        settlement?;
        history
    }
}

struct ChatPaymentTransport {
    actor: Arc<NativeChatActor>,
    context: NativeChatContext,
    peer_identity: [u8; 32],
}

#[async_trait::async_trait]
impl PaymentTransport for ChatPaymentTransport {
    async fn accept(
        &self,
        payment: &HostNativeChatPayment,
        memo: truapi_coinage::TransferMemo,
    ) -> Result<(), ()> {
        // Acceptance is irreversible. Rewrapping changes only the encrypted
        // delivery to an authenticated roster, never the payment or reservation.
        let accepted = match self
            .actor
            .store
            .read(|state| state.accepted_payments.contains(&payment.operation_id))
            .await
        {
            Ok(accepted) => accepted,
            Err(_) => return Ok(()), // unknown durable state: retain reservation
        };
        let rejected = || if accepted { Ok(()) } else { Err(()) };
        if !(self.context.session_valid)() || payment.peer_identity != self.peer_identity {
            return rejected();
        }
        let peer = match self
            .actor
            .store
            .read(|state| state.peer(&self.peer_identity).cloned())
            .await
        {
            Ok(Ok(peer)) if peer.ready_for_payments() => peer,
            _ => return rejected(),
        };
        let keys = Zeroizing::new(
            memo.entries
                .iter()
                .map(|entry| entry.0.to_vec())
                .collect::<Vec<_>>(),
        );
        let raw = match wire::encode_coinage_send_message(
            &payment.message_id,
            payment.timestamp,
            &memo.total_value.to_string(),
            &keys,
        ) {
            Ok(raw) => raw,
            Err(_) => return rejected(),
        };
        let messages = Zeroizing::new(vec![raw]);
        let payment_id = payment.operation_id;
        let request_id = format!("pay-{}", hex::encode(payment_id));
        let encoded = Zeroizing::new(messages.encode());
        let digest = hash(&encoded);
        if accepted {
            let unchanged = self
                .actor
                .store
                .read(|state| {
                    state
                        .outbox
                        .iter()
                        .find(|entry| entry.kind == OutgoingKind::Payment(payment_id))
                        .is_none_or(|entry| {
                            entry.peer == peer.identity
                                && entry.request_id == request_id
                                && entry.digest == digest
                                && entry.roster_revision == peer.revision
                        })
                })
                .await
                .unwrap_or(true);
            if unchanged {
                self.actor.start_delivery(&self.context);
                return Ok(());
            }
        }
        let actor = self.actor.clone();
        let valid = self.context.session_valid.clone();
        let write_attempted = Arc::new(AtomicBool::new(accepted));
        let attempted = write_attempted.clone();
        let outcome = self
            .actor
            .store
            .update(move |state| {
                if !valid() {
                    return Err(Error::NotConnected);
                }
                let current = state.peer(&peer.identity)?.clone();
                if !current.ready_for_payments() || current.revision != peer.revision {
                    return Err(Error::PeerNotReady);
                }
                if state.accepted_payments.contains(&payment_id) {
                    // Absence means its peer ACK was already durably recorded.
                    let Some(position) = state
                        .outbox
                        .iter()
                        .position(|entry| entry.kind == OutgoingKind::Payment(payment_id))
                    else {
                        return Ok(());
                    };
                    let previous = &state.outbox[position];
                    if previous.peer != peer.identity
                        || previous.request_id != request_id
                        || previous.digest != digest
                    {
                        return Err(Error::OperationConflict);
                    }
                    if previous.roster_revision == current.revision {
                        return Ok(());
                    }
                    let statement = actor.multi_statement(
                        state,
                        &current,
                        &current.payment_devices().collect::<Vec<_>>(),
                        &request_id,
                        &messages,
                    )?;
                    let outgoing = &mut state.outbox[position];
                    outgoing.statement = statement;
                    outgoing.roster_revision = current.revision;
                    outgoing.last_attempt = 0;
                } else {
                    if state.accepted_payments.len() >= MAX_RECEIPTS {
                        return Err(Error::StorageUnavailable);
                    }
                    let statement = actor.multi_statement(
                        state,
                        &current,
                        &current.payment_devices().collect::<Vec<_>>(),
                        &request_id,
                        &messages,
                    )?;
                    state.queue(Outgoing {
                        peer: peer.identity,
                        request_id,
                        digest,
                        kind: OutgoingKind::Payment(payment_id),
                        roster_revision: current.revision,
                        statement,
                        last_attempt: 0,
                    })?;
                    state.accepted_payments.push(payment_id);
                }
                attempted.store(true, Ordering::Release);
                Ok(())
            })
            .await;
        if outcome.is_err() && !write_attempted.load(Ordering::Acquire) {
            return Err(());
        }
        self.actor.start_delivery(&self.context);
        Ok(())
    }
}

fn random_bytes<const N: usize>() -> Result<[u8; N], Error> {
    let mut bytes = [0; N];
    getrandom::getrandom(&mut bytes).map_err(|_| Error::StorageUnavailable)?;
    Ok(bytes)
}
fn random_id() -> Result<String, Error> {
    Ok(hex::encode(random_bytes::<16>()?))
}
fn hash(bytes: &[u8]) -> [u8; 32] {
    sp_crypto_hashing::blake2_256(bytes)
}
fn valid_id(value: &str) -> Result<(), Error> {
    if value.is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
        Err(Error::InvalidRequest)
    } else {
        Ok(())
    }
}
fn fresh(timestamp_ms: u64, now: u64) -> bool {
    let timestamp = timestamp_ms / 1000;
    timestamp >= wire::PROTOCOL_EPOCH_SECONDS
        && timestamp <= now.saturating_add(CLOCK_SKEW)
        && now <= timestamp.saturating_add(LIFETIME)
}
