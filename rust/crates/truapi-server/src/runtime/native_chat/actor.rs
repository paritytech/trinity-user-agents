// SPDX-License-Identifier: AGPL-3.0-only
//! Non-exportable native Chat cryptography and recipient-bound payment custody.
//! Products own ordinary protocol, history, delivery, retries and acknowledgment.

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
    chat_device::{HostChatDevice, PeerDevice},
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
    fn new(identity: [u8; 32], root_key: [u8; 32], username: Option<String>) -> Self {
        Self {
            identity,
            root_key,
            username,
            devices: Vec::new(),
            invitation: None,
            invitation_text: None,
            invitation_timestamp: None,
            established: false,
            revocation_request: None,
            revocation_acked: false,
            revocation_acks: Vec::new(),
            revision: 0,
        }
    }
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

impl Outgoing {
    fn prepared(&self, state: &State) -> HostNativeChatPrepared {
        HostNativeChatPrepared {
            statement: self.statement.clone(),
            peer_identity: self.peer,
            request_id: self.request_id.clone(),
            requires_ack: self.kind != OutgoingKind::Acknowledgment,
            client_request_id: state
                .sent
                .iter()
                .find(|sent| sent.peer == self.peer && sent.wire_request_id == self.request_id)
                .map(|sent| sent.request_id.clone()),
        }
    }
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

#[derive(Clone, Encode)]
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
    marker: [u8; 4],
    boundary: BoundaryState,
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
            marker: *b"HCN3",
            boundary: BoundaryState::default(),
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
            return Ok(());
        }
        if self.outbox.len() >= MAX_OUTBOX {
            return Err(Error::StorageUnavailable);
        }
        self.outbox.push(outgoing);
        Ok(())
    }
}

impl Decode for State {
    fn decode<I: parity_scale_codec::Input>(
        input: &mut I,
    ) -> Result<Self, parity_scale_codec::Error> {
        let secret = <Secret32>::decode(input)?;
        let index = <[u8; 32]>::decode(input)?;
        let peers = <Vec<Peer>>::decode(input)?;
        let invitations = <Vec<Invitation>>::decode(input)?;
        let outbox = <Vec<Outgoing>>::decode(input)?;
        let received = <Vec<Receipt>>::decode(input)?;
        let sent = <Vec<SentReceipt>>::decode(input)?;
        let accepted_payments = <Vec<[u8; 32]>>::decode(input)?;
        let payment_acknowledgments = <Vec<[u8; 32]>>::decode(input)?;
        let messages = <Vec<HostNativeChatMessages>>::decode(input)?;
        let acknowledgments = <Vec<HostNativeChatAcknowledgment>>::decode(input)?;
        let last_expiry = <u64>::decode(input)?;
        let history_imports = <Vec<history::HistoryImport>>::decode(input)?;
        let files = <Vec<files::FileRecord>>::decode(input)?;
        let rich_messages = <Vec<files::RichRecord>>::decode(input)?;
        // The original snapshot ends exactly here. Never reinterpret malformed
        // new extensions as old state, and never discard a legacy custody slot.
        let boundary = if input.remaining_len()? == Some(0) {
            BoundaryState {
                legacy_pending: true,
                ..BoundaryState::default()
            }
        } else {
            if <[u8; 4]>::decode(input)? != *b"HCN3" {
                return Err("invalid Chat boundary state".into());
            }
            BoundaryState::decode(input)?
        };
        Ok(Self {
            secret,
            index,
            peers,
            invitations,
            outbox,
            received,
            sent,
            accepted_payments,
            payment_acknowledgments,
            messages,
            acknowledgments,
            last_expiry,
            history_imports,
            files,
            rich_messages,
            marker: *b"HCN3",
            boundary,
        })
    }
}

#[derive(Clone, Default, Encode, Decode)]
struct BoundaryState {
    legacy_pending: bool,
    migration: Option<MigrationSnapshot>,
    committed_migration: Option<[u8; 32]>,
    history: Vec<history::HistoryDelivery>,
    incoming: Vec<IncomingRequest>,
}

/// Bounded authentication evidence for an ACK, never ordinary message content.
/// In particular, a departure can be acknowledged to its authenticated sender
/// after that sender has removed itself from the active payment roster.
#[derive(Clone, Encode, Decode)]
struct IncomingRequest {
    peer: [u8; 32],
    request_id: String,
    digest: [u8; 32],
    sender: [u8; 32],
    key: [u8; 32],
    route: HostNativeChatRoute,
    timestamp: u64,
}

#[derive(Clone, Encode, Decode)]
struct MigrationSnapshot {
    id: [u8; 32],
    payments: Vec<HostNativeChatPayment>,
}
pub(super) struct NativeChatActor {
    product: String,
    public: HostNativeChatDevice,
    legacy_account: [u8; 32],
    root_secret: Zeroizing<[u8; 32]>,
    signer: Keypair,
    device: HostChatDevice,
    store: Arc<ChatStateStore<State>>,
    receiving: futures::lock::Mutex<()>,
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
            receiving: futures::lock::Mutex::new(()),
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
            || state.boundary.incoming.len() > MAX_RECEIPTS
        {
            return Err(Error::StorageUnavailable);
        }
        history::validate_imports(&state.history_imports)?;
        history::validate_deliveries(&state.boundary.history)?;
        files::validate(state)?;
        for incoming in &state.boundary.incoming {
            state
                .peer(&incoming.peer)
                .map_err(|_| Error::StorageUnavailable)?;
            valid_id(&incoming.request_id).map_err(|_| Error::StorageUnavailable)?;
            self.validate_remote_device(&PeerDevice {
                account_id: incoming.sender,
                public_key: incoming.key,
            })
            .map_err(|_| Error::StorageUnavailable)?;
        }
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

    fn peer_views(&self, state: &State) -> Result<Vec<HostNativeChatPeer>, Error> {
        state
            .peers
            .iter()
            .map(|peer| {
                Ok(HostNativeChatPeer {
                    identity_account_id: peer.identity,
                    username: peer.username.clone(),
                    devices: peer
                        .active_devices()
                        .into_iter()
                        .map(|device| HostNativeChatPeerDevice {
                            account_id: device.account_id,
                            chat_public_key: device.public_key,
                        })
                        .collect(),
                    incoming_channels: self.peer_incoming_topics(peer)?,
                    ready_for_payments: peer.ready_for_payments(),
                })
            })
            .collect()
    }

    async fn require_migrated(&self) -> Result<(), Error> {
        self.store
            .read(|state| {
                if state.boundary.legacy_pending {
                    Err(Error::OperationConflict)
                } else {
                    Ok(())
                }
            })
            .await?
    }

    fn legacy_view(
        &self,
        state: &State,
        payments: Vec<HostNativeChatPayment>,
    ) -> Result<truapi::v02::HostProductDeviceChatResponse, Error> {
        Ok(truapi::v02::HostProductDeviceChatResponse {
            device: self.public.clone(),
            peers: self.peer_views(state)?,
            invitations: state
                .invitations
                .iter()
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
    }

    pub(super) async fn public_view(
        self: &Arc<Self>,
        context: &NativeChatContext,
        payments: Vec<HostNativeChatPayment>,
    ) -> Result<HostProductDeviceChatResponse, Error> {
        context.require_current()?;
        if self
            .store
            .read(|state| state.boundary.legacy_pending && state.boundary.migration.is_none())
            .await?
        {
            let payments = payments.clone();
            let actor = self.clone();
            let valid = context.session_valid.clone();
            self.store
                .update(move |state| {
                    if !valid() {
                        return Err(Error::NotConnected);
                    }
                    if state.boundary.legacy_pending && state.boundary.migration.is_none() {
                        let view = actor.legacy_view(state, payments.clone())?;
                        let statements: Vec<_> = state
                            .outbox
                            .iter()
                            .filter(|entry| !matches!(entry.kind, OutgoingKind::Payment(_)))
                            .map(|entry| entry.prepared(state))
                            .collect();
                        let invitations: Vec<_> = state
                            .invitations
                            .iter()
                            .map(|invitation| HostNativeChatMigrationInvitation {
                                invitation_id: invitation.id,
                                request_id: invitation.message_id.clone(),
                            })
                            .collect();
                        let id = hash(
                            &(
                                b"native-chat-migration-v3",
                                &view,
                                &statements,
                                &invitations,
                            )
                                .encode(),
                        );
                        // Legacy fields themselves are frozen until commit. Do not copy
                        // the entire old history/ciphertext into its own snapshot.
                        state.boundary.migration = Some(MigrationSnapshot { id, payments });
                    }
                    Ok(())
                })
                .await?;
        }
        self.store
            .read(|state| {
                let prepared = state
                    .outbox
                    .iter()
                    .filter(|entry| {
                        state.boundary.legacy_pending
                            || matches!(
                                entry.kind,
                                OutgoingKind::Payment(_) | OutgoingKind::Rich(_)
                            )
                    })
                    .map(|entry| entry.prepared(state))
                    .collect();
                Ok(HostProductDeviceChatResponse {
                    device: self.public.clone(),
                    peers: self.peer_views(state)?,
                    binding: None,
                    opened: Vec::new(),
                    prepared,
                    payments,
                    rich_messages: files::public_views(state)?,
                    migration: state
                        .boundary
                        .migration
                        .as_ref()
                        .map(|migration| self.legacy_view(state, migration.payments.clone()))
                        .transpose()?,
                    migration_id: state
                        .boundary
                        .migration
                        .as_ref()
                        .map(|migration| migration.id),
                    migration_invitations: if state.boundary.legacy_pending {
                        state
                            .invitations
                            .iter()
                            .map(|invitation| HostNativeChatMigrationInvitation {
                                invitation_id: invitation.id,
                                request_id: invitation.message_id.clone(),
                            })
                            .collect()
                    } else {
                        Vec::new()
                    },
                    open_page: None,
                    state_page: None,
                    coinage_cents_unit: None,
                })
            })
            .await?
    }

    pub(super) async fn commit_migration(
        self: &Arc<Self>,
        context: &NativeChatContext,
        migration_id: [u8; 32],
    ) -> Result<(), Error> {
        let valid = context.session_valid.clone();
        self.store
            .update(move |state| {
                if !valid() {
                    return Err(Error::NotConnected);
                }
                if state.boundary.committed_migration == Some(migration_id) {
                    return Ok(());
                }
                let migration = state
                    .boundary
                    .migration
                    .as_ref()
                    .ok_or(Error::OperationNotFound)?;
                if migration.id != migration_id {
                    return Err(Error::OperationConflict);
                }
                // Only the explicitly transferred ordinary view/ciphertext is retired.
                // Installation keys, payment custody and real file progress survive.
                state
                    .outbox
                    .retain(|entry| matches!(entry.kind, OutgoingKind::Payment(_)));
                state.messages.clear();
                state.acknowledgments.clear();
                state.sent.clear();
                state
                    .received
                    .retain(|entry| entry.request_id.starts_with("invite:"));
                for peer in &mut state.peers {
                    peer.invitation_text = None;
                }
                for invitation in &mut state.invitations {
                    invitation.text.clear();
                }
                state.boundary.legacy_pending = false;
                state.boundary.migration = None;
                state.boundary.committed_migration = Some(migration_id);
                Ok(())
            })
            .await?;
        self.acknowledge_history(context).await
    }

    pub(super) async fn bind(
        self: &Arc<Self>,
        context: &NativeChatContext,
        username: String,
    ) -> Result<HostNativeChatBinding, Error> {
        self.require_migrated().await?;
        let resolved = identity::resolve_username(context, &username).await?;
        context.require_current()?;
        if resolved.identity_account_id == self.public.identity_account_id {
            return Err(Error::InvalidRequest);
        }
        let shared = chat_shared_secret(&self.root_secret, &resolved.chat_public_key)
            .map_err(|_| Error::InvalidStatement)?;
        let binding = HostNativeChatBinding {
            peer_identity: resolved.identity_account_id,
            peer_chat_public_key: resolved.chat_public_key,
            identity_proof: chat_device_identity_proof(
                &shared,
                &self.public.identity_account_id,
                &self.public.account_id,
            ),
        };
        let valid = context.session_valid.clone();
        self.store
            .update(move |state| {
                if !valid() {
                    return Err(Error::NotConnected);
                }
                match state
                    .peers
                    .iter_mut()
                    .find(|peer| peer.identity == resolved.identity_account_id)
                {
                    Some(peer) => {
                        if peer.root_key != resolved.chat_public_key {
                            return Err(Error::InvalidStatement);
                        }
                        peer.username = resolved.username;
                    }
                    None => {
                        if state.peers.len() >= MAX_PEERS {
                            return Err(Error::StorageUnavailable);
                        }
                        state.peers.push(Peer::new(
                            resolved.identity_account_id,
                            resolved.chat_public_key,
                            resolved.username,
                        ));
                    }
                }
                Ok(())
            })
            .await?;
        Ok(binding)
    }

    pub(super) async fn prepare(
        self: &Arc<Self>,
        context: &NativeChatContext,
        identity: [u8; 32],
        route: HostNativeChatRoute,
        plaintext: Vec<u8>,
    ) -> Result<Vec<HostNativeChatPrepared>, Error> {
        let plaintext = Zeroizing::new(plaintext);
        self.require_migrated().await?;
        context.require_current()?;
        if plaintext.len() > crate::runtime::chat_device::MAX_CHAT_ENVELOPE_BYTES {
            return Err(Error::InvalidRequest);
        }
        let _gate = self.receiving.lock().await;
        let actor = self.clone();
        let valid = context.session_valid.clone();
        let (request_id, requires_ack, acknowledgment) = if route == HostNativeChatRoute::Invitation
        {
            let message = wire::decode_chat_request_message_v2(&plaintext)
                .map_err(|_| Error::InvalidRequest)?;
            (message.message_id, true, None)
        } else {
            match crate::runtime::chat_device::open_identity_exchange(&plaintext)
                .map_err(|_| Error::InvalidRequest)?
            {
                crate::runtime::chat_device::OpenedDeviceExchange::Response {
                    request_id,
                    response_code,
                } => (
                    request_id.clone(),
                    false,
                    (response_code == 0).then_some(request_id),
                ),
                crate::runtime::chat_device::OpenedDeviceExchange::Request {
                    request_id, ..
                } => (request_id, true, None),
            }
        };
        let statement = self
            .store
            .update(move |state| {
                if !valid() {
                    return Err(Error::NotConnected);
                }
                let mut peer = state.peer(&identity)?.clone();
                if route == HostNativeChatRoute::Invitation {
                    let message = wire::decode_chat_request_message_v2(&plaintext)
                        .map_err(|_| Error::InvalidRequest)?;
                    valid_id(&message.message_id)?;
                    let shared = chat_shared_secret(&actor.root_secret, &peer.root_key)
                        .map_err(|_| Error::InvalidStatement)?;
                    if !fresh(message.timestamp, current_unix_secs())
                        || message.content.identity_proof.identity_account_id
                            != actor.public.identity_account_id
                        || message.content.identity_proof.proof
                            != chat_device_identity_proof(
                                &shared,
                                &actor.public.identity_account_id,
                                &actor.public.account_id,
                            )
                        || message.content.device_enc_pub_key != actor.public.chat_public_key
                        || message
                            .content
                            .welcome_text
                            .as_ref()
                            .is_some_and(|text| text.len() > 8192)
                    {
                        return Err(Error::InvalidRequest);
                    }
                    if peer
                        .invitation
                        .as_ref()
                        .is_some_and(|id| id != &message.message_id)
                    {
                        return Err(Error::OperationConflict);
                    }
                    let payload = wire::encode_chat_request_v2_proof_payload(&message, &identity)
                        .map_err(|_| Error::InvalidRequest)?;
                    let signature = actor
                        .signer
                        .secret
                        .sign_simple(SR25519_SIGNING_CONTEXT, &payload, &actor.signer.public)
                        .to_bytes();
                    peer.invitation = Some(message.message_id.clone());
                    peer.invitation_timestamp = Some(message.timestamp);
                    let day = wire::chat_request_day_from_unix(message.timestamp / 1000)
                        .ok_or(Error::InvalidRequest)?;
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
                    *state.peer_mut(&identity)? = peer;
                    return actor.sign(
                        state,
                        chat_request_channel_id(
                            &shared,
                            &actor.public.identity_account_id,
                            &identity,
                        ),
                        vec![
                            wire::chat_request_full_topic(&identity),
                            wire::chat_request_day_topic(&identity, day),
                        ],
                        data,
                    );
                }
                let response = actor.authorize_prepare(state, &mut peer, route, &plaintext)?;
                *state.peer_mut(&identity)? = peer.clone();
                actor.transport_statement(state, &peer, route, response, &plaintext)
            })
            .await?;
        if let Some(request_id) = acknowledgment {
            // Product constructs this only after persisting every history page and
            // completing every incoming top-up. Reads never consume a HOP page.
            self.commit_history(context, identity, &request_id).await?;
        }
        Ok(vec![HostNativeChatPrepared {
            statement,
            peer_identity: identity,
            request_id,
            requires_ack,
            client_request_id: None,
        }])
    }

    fn transport_statement(
        &self,
        state: &mut State,
        peer: &Peer,
        route: HostNativeChatRoute,
        response: bool,
        plaintext: &[u8],
    ) -> Result<SignedStatement, Error> {
        let recipients = if response && route == HostNativeChatRoute::Device {
            let wire::V2StatementTransportData::Response { request_id, .. } =
                wire::decode_transport_plaintext(plaintext).map_err(|_| Error::InvalidRequest)?
            else {
                return Err(Error::InvalidRequest);
            };
            let mut devices = Vec::new();
            for incoming in state.boundary.incoming.iter().filter(|entry| {
                entry.peer == peer.identity
                    && entry.request_id == request_id
                    && entry.route == route
            }) {
                if !devices
                    .iter()
                    .any(|device: &PeerDevice| device.account_id == incoming.sender)
                {
                    devices.push(PeerDevice {
                        account_id: incoming.sender,
                        public_key: incoming.key,
                    });
                }
            }
            if devices.is_empty() {
                peer.active_devices()
            } else {
                devices
            }
        } else {
            peer.active_devices()
        };
        let (shared, sender, body) = match route {
            HostNativeChatRoute::Identity => (
                chat_shared_secret(&self.root_secret, &peer.root_key)
                    .map_err(|_| Error::InvalidStatement)?,
                self.public.identity_account_id,
                Zeroizing::new(plaintext.to_vec()),
            ),
            HostNativeChatRoute::Device => (
                self.device
                    .identity_shared_secret(&peer.root_key)
                    .map_err(|_| Error::InvalidStatement)?,
                self.public.account_id,
                Zeroizing::new(
                    self.device
                        .seal_multi_device(&recipients, plaintext)
                        .map_err(|_| Error::InvalidStatement)?,
                ),
            ),
            HostNativeChatRoute::Invitation => return Err(Error::InvalidRequest),
        };
        let session = chat_identity_session_id(&shared, &sender, &peer.identity);
        let channel = if response {
            wire::chat_identity_response_topic(&session)
        } else {
            wire::chat_identity_request_topic(&session)
        }
        .map_err(|_| Error::InvalidStatement)?;
        let encrypted = native_root_seal(&shared, &body).map_err(|_| Error::InvalidStatement)?;
        self.sign(state, channel, vec![session], encrypted)
    }

    pub(super) async fn payment(
        self: &Arc<Self>,
        context: &NativeChatContext,
        peer_identity: [u8; 32],
        request_id: String,
        amount_cents: u64,
    ) -> Result<(PaymentIntent, Arc<dyn PaymentTransport>), Error> {
        self.require_migrated().await?;
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

    pub(super) async fn reconcile(
        self: &Arc<Self>,
        context: &NativeChatContext,
        registry: &NativeChatRegistry,
    ) -> Result<(), Error> {
        context.require_current()?;
        self.store.reauthenticate().await?;
        let (accepted, required) = self
            .store
            .read(|state| {
                (
                    state.accepted_payments.clone(),
                    !state.accepted_payments.is_empty()
                        || !state.payment_acknowledgments.is_empty()
                        || state
                            .outbox
                            .iter()
                            .any(|entry| matches!(entry.kind, OutgoingKind::Payment(_))),
                )
            })
            .await?;
        if let Some(wallet) = registry.existing_wallet(context, required).await? {
            self.replay_payment_acknowledgments(context, registry)
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
            wallet.reconcile(context).await?;
        }
        // Only expiry may be renewed on a durable opaque payment handoff. The
        // product submits/retries the resulting statement; Host never delivers.
        let pending = self
            .store
            .read(|state| {
                state
                    .outbox
                    .iter()
                    .filter(|entry| {
                        matches!(entry.kind, OutgoingKind::Payment(_))
                            || (!state.boundary.legacy_pending
                                && matches!(entry.kind, OutgoingKind::Rich(_)))
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .await?;
        for outgoing in pending {
            if outgoing
                .statement
                .expiry
                .is_none_or(|expiry| (expiry >> 32) <= current_unix_secs())
            {
                self.refresh_statement(outgoing.peer, &outgoing.request_id, &outgoing.kind, 0)
                    .await?;
            }
        }
        Ok(())
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
        Ok(())
    }
}

fn random_bytes<const N: usize>() -> Result<[u8; N], Error> {
    let mut bytes = [0; N];
    getrandom::getrandom(&mut bytes).map_err(|_| Error::StorageUnavailable)?;
    Ok(bytes)
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
