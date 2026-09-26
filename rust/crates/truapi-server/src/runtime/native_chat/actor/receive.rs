// SPDX-License-Identifier: AGPL-3.0-only
//! Authenticate complete external native statements before exposing plaintext.

use super::*;
use crate::host_logic::statement_store::{
    decode_verified_statement_data, statement_expiry_elapsed,
};
use crate::runtime::chat_device::{
    DeviceControl, DeviceLifecycle, OpenedDeviceExchange, OpenedDeviceMessage,
    open_identity_exchange,
};
use schnorrkel::{PublicKey, Signature};

type OpenResult = (Vec<HostNativeChatOpened>, Option<HostNativeChatOpenPage>);

impl NativeChatActor {
    pub(in crate::runtime::native_chat) async fn open_statement(
        self: &Arc<Self>,
        context: &NativeChatContext,
        registry: &NativeChatRegistry,
        statement: SignedStatement,
    ) -> Result<OpenResult, Error> {
        self.require_migrated().await?;
        let _incoming = self.receiving.lock().await;
        context.require_current()?;
        if statement
            .data
            .as_ref()
            .is_none_or(|data| data.len() > 256 * 1024)
            || statement.topics.len() > 8
        {
            return Err(Error::InvalidStatement);
        }
        let encoded =
            signed_statement_to_scale(statement.clone()).map_err(|_| Error::InvalidStatement)?;
        let verified =
            decode_verified_statement_data(&encoded, None).map_err(|_| Error::InvalidStatement)?;
        if verified
            .expiry
            .is_none_or(|expiry| statement_expiry_elapsed(expiry, current_unix_secs()))
            || verified.signer == self.public.account_id
            || verified.signer == self.legacy_account
            || verified.signer == self.public.identity_account_id
        {
            // This check precedes EVERY decryption route. In particular, never
            // return outgoing main-purse plaintext by reflecting Host output.
            return Err(Error::InvalidStatement);
        }
        if statement.topics.contains(&wire::chat_request_full_topic(
            &self.public.identity_account_id,
        )) {
            let opened = self
                .open_invitation(context, statement, verified.signer, verified.data)
                .await?;
            return Ok((vec![opened], None));
        }
        let channel = statement.channel.ok_or(Error::InvalidStatement)?;
        let peers = self.store.read(|state| state.peers.clone()).await?;
        for peer in peers {
            let root_shared = chat_shared_secret(&self.root_secret, &peer.root_key)
                .map_err(|_| Error::InvalidStatement)?;
            let root_incoming = chat_identity_session_id(
                &root_shared,
                &peer.identity,
                &self.public.identity_account_id,
            );
            let root_request = statement.topics.contains(&root_incoming)
                && wire::chat_identity_request_topic(&root_incoming).ok() == Some(channel);
            let root_response = statement.topics.contains(&root_incoming)
                && wire::chat_identity_response_topic(&root_incoming).ok() == Some(channel);
            if root_request || root_response {
                let plaintext = native_root_open(&root_shared, &verified.data)
                    .map_err(|_| Error::InvalidStatement)?;
                let exchange =
                    open_identity_exchange(&plaintext).map_err(|_| Error::InvalidStatement)?;
                let admitted = peer
                    .active_devices()
                    .into_iter()
                    .find(|device| device.account_id == verified.signer);
                let sender = admitted
                    .or_else(|| match &exchange {
                        OpenedDeviceExchange::Request { messages, .. } if root_request => {
                            messages.iter().find_map(|message| match message {
                                OpenedDeviceMessage::DeviceControl(DeviceControl {
                                    content: DeviceLifecycle::MultiAccepted { request_id, device },
                                    ..
                                }) if peer.invitation.as_deref() == Some(request_id)
                                    && device.account_id == verified.signer =>
                                {
                                    Some(*device)
                                }
                                _ => None,
                            })
                        }
                        _ => None,
                    })
                    .ok_or(Error::InvalidStatement)?;
                return self
                    .open_exchange(
                        context,
                        registry,
                        peer,
                        sender,
                        HostNativeChatRoute::Identity,
                        root_request,
                        plaintext,
                        exchange,
                    )
                    .await;
            }
            if !peer.established {
                continue;
            }
            let Some(sender) = peer
                .active_devices()
                .into_iter()
                .find(|device| device.account_id == verified.signer)
            else {
                continue;
            };
            let shared = chat_shared_secret(&self.root_secret, &sender.public_key)
                .map_err(|_| Error::InvalidStatement)?;
            let incoming = chat_identity_session_id(
                &shared,
                &sender.account_id,
                &self.public.identity_account_id,
            );
            let request = statement.topics.contains(&incoming)
                && wire::chat_identity_request_topic(&incoming).ok() == Some(channel);
            let response = statement.topics.contains(&incoming)
                && wire::chat_identity_response_topic(&incoming).ok() == Some(channel);
            if !request && !response {
                continue;
            }
            let envelope =
                native_root_open(&shared, &verified.data).map_err(|_| Error::InvalidStatement)?;
            let (plaintext, exchange) = self
                .device
                .open_multi_device_plaintext(&sender, &envelope)
                .map_err(|_| Error::InvalidStatement)?;
            return self
                .open_exchange(
                    context,
                    registry,
                    peer,
                    sender,
                    HostNativeChatRoute::Device,
                    request,
                    plaintext,
                    exchange,
                )
                .await;
        }
        Err(Error::InvalidStatement)
    }

    #[allow(clippy::too_many_arguments)]
    async fn open_exchange(
        self: &Arc<Self>,
        context: &NativeChatContext,
        registry: &NativeChatRegistry,
        peer: Peer,
        sender: PeerDevice,
        route: HostNativeChatRoute,
        request_route: bool,
        mut plaintext: Zeroizing<Vec<u8>>,
        exchange: OpenedDeviceExchange,
    ) -> Result<OpenResult, Error> {
        self.validate_remote_device(&sender)?;
        match exchange {
            OpenedDeviceExchange::Response {
                request_id,
                response_code,
            } if !request_route => {
                self.receive_acknowledgment(
                    context,
                    registry,
                    peer.identity,
                    sender.account_id,
                    request_id,
                    response_code,
                )
                .await?;
                Ok((
                    vec![HostNativeChatOpened {
                        peer_identity: peer.identity,
                        sender_account_id: sender.account_id,
                        route,
                        plaintext: core::mem::take(&mut *plaintext),
                    }],
                    None,
                ))
            }
            OpenedDeviceExchange::Request {
                request_id,
                messages,
            } if request_route => {
                let digest = exchange_digest(&messages)?;
                let mut controls = Vec::new();
                for message in messages {
                    if let OpenedDeviceMessage::DeviceControl(control) = message {
                        if !valid_peer_timestamp(control.timestamp, current_unix_secs()) {
                            return Err(Error::InvalidStatement);
                        }
                        match &control.content {
                            DeviceLifecycle::Added(device)
                            | DeviceLifecycle::MultiAccepted { device, .. } => {
                                self.validate_remote_device(device)?
                            }
                            _ => (),
                        }
                        controls.push(control);
                    }
                }
                let has_controls = !controls.is_empty();
                controls.sort_by(|a, b| {
                    (a.timestamp, &a.message_id).cmp(&(b.timestamp, &b.message_id))
                });
                let admitted = peer.active_devices().contains(&sender);
                let last_departure = controls
                    .iter()
                    .filter_map(|control| {
                        matches!(control.content, DeviceLifecycle::LeftChat)
                            .then_some(control.timestamp)
                    })
                    .max();
                let original_revision = peer.revision;
                let original_invitation = peer.invitation.clone();
                let mut prospective = peer.clone();
                for control in controls {
                    apply_control(&mut prospective, control, &sender, admitted, last_departure)?;
                }
                if !admitted && (original_invitation.is_none() || prospective.invitation.is_some())
                {
                    return Err(Error::InvalidStatement);
                }
                if prospective.revision != original_revision {
                    prospective.revocation_acks.clear();
                    prospective.revocation_acked = false;
                    prospective.revocation_request = None;
                }
                let identity = peer.identity;
                let valid = context.session_valid.clone();
                let receipt_id = request_id.clone();
                self.store
                    .update(move |state| {
                        if !valid() {
                            return Err(Error::NotConnected);
                        }
                        let now = current_unix_secs();
                        state
                            .boundary
                            .incoming
                            .retain(|entry| now <= entry.timestamp.saturating_add(LIFETIME));
                        if state.boundary.incoming.iter().any(|entry| {
                            entry.peer == identity
                                && entry.request_id == receipt_id
                                && entry.digest != digest
                        }) {
                            return Err(Error::InvalidStatement);
                        }
                        if !state.boundary.incoming.iter().any(|entry| {
                            entry.peer == identity
                                && entry.request_id == receipt_id
                                && entry.sender == sender.account_id
                                && entry.route == route
                        }) {
                            if state.boundary.incoming.len() >= MAX_RECEIPTS {
                                return Err(Error::StorageUnavailable);
                            }
                            state.boundary.incoming.push(IncomingRequest {
                                peer: identity,
                                request_id: receipt_id.clone(),
                                digest,
                                sender: sender.account_id,
                                key: sender.public_key,
                                route,
                                timestamp: now,
                            });
                        }
                        if has_controls {
                            if let Some(receipt) = state.received.iter().find(|entry| {
                                entry.peer == identity && entry.request_id == receipt_id
                            }) {
                                if receipt.digest != digest {
                                    return Err(Error::InvalidStatement);
                                }
                                return Ok(());
                            }
                            state.expire_receipts(current_unix_secs());
                            if state.received.len() >= MAX_RECEIPTS {
                                return Err(Error::StorageUnavailable);
                            }
                            let current = state.peer(&identity)?;
                            if current.revision != original_revision
                                || current.invitation != original_invitation
                            {
                                return Err(Error::InvalidStatement);
                            }
                            *state.peer_mut(&identity)? = prospective;
                            state.received.push(Receipt {
                                peer: identity,
                                request_id: receipt_id,
                                digest,
                                timestamp: current_unix_secs(),
                            });
                        }
                        Ok(())
                    })
                    .await?;
                self.open_history(
                    context,
                    identity,
                    sender.account_id,
                    route,
                    request_id,
                    plaintext,
                )
                .await
            }
            _ => Err(Error::InvalidStatement),
        }
    }

    pub(super) fn validate_remote_device(&self, device: &PeerDevice) -> Result<(), Error> {
        if device.account_id == self.public.account_id
            || device.account_id == self.legacy_account
            || device.account_id == self.public.identity_account_id
            || device.public_key == self.public.chat_public_key
            || device.public_key == self.public.identity_chat_public_key
        {
            return Err(Error::InvalidStatement);
        }
        self.device
            .identity_shared_secret(&device.public_key)
            .map_err(|_| Error::InvalidStatement)?;
        Ok(())
    }

    async fn open_invitation(
        self: &Arc<Self>,
        context: &NativeChatContext,
        statement: SignedStatement,
        signer: [u8; 32],
        data: Vec<u8>,
    ) -> Result<HostNativeChatOpened, Error> {
        if wire::is_context_bound_chat_request_v2(&data) {
            return Err(Error::InvalidStatement);
        }
        let request = wire::open_chat_request_v2(&self.root_secret, &data)
            .map_err(|_| Error::InvalidStatement)?;
        let message = &request.message;
        let peer_identity = message.content.identity_proof.identity_account_id;
        let now = current_unix_secs();
        valid_id(&message.message_id).map_err(|_| Error::InvalidStatement)?;
        if peer_identity == self.public.identity_account_id
            || peer_identity == [0; 32]
            || request.proof.signer.as_slice() != signer
            || !fresh(message.timestamp, now)
            || message
                .content
                .welcome_text
                .as_ref()
                .is_some_and(|text| text.len() > 8192)
        {
            return Err(Error::InvalidStatement);
        }
        let day = wire::chat_request_day_from_unix(message.timestamp / 1000)
            .ok_or(Error::InvalidStatement)?;
        if !statement.topics.contains(&wire::chat_request_day_topic(
            &self.public.identity_account_id,
            day,
        )) {
            return Err(Error::InvalidStatement);
        }
        let payload =
            wire::encode_chat_request_v2_proof_payload(message, &self.public.identity_account_id)
                .map_err(|_| Error::InvalidStatement)?;
        PublicKey::from_bytes(&signer)
            .map_err(|_| Error::InvalidStatement)?
            .verify_simple(
                SR25519_SIGNING_CONTEXT,
                &payload,
                &Signature::from_bytes(&request.proof.signature)
                    .map_err(|_| Error::InvalidStatement)?,
            )
            .map_err(|_| Error::InvalidStatement)?;
        self.device
            .identity_shared_secret(&message.content.device_enc_pub_key)
            .map_err(|_| Error::InvalidStatement)?;
        let resolved = identity::resolve_account(context, peer_identity).await?;
        let shared = chat_shared_secret(&self.root_secret, &resolved.chat_public_key)
            .map_err(|_| Error::InvalidStatement)?;
        if !verify_chat_device_identity_proof(
            &shared,
            &peer_identity,
            &signer,
            &message.content.identity_proof.proof,
        ) || statement.channel
            != Some(chat_request_channel_id(
                &shared,
                &peer_identity,
                &self.public.identity_account_id,
            ))
        {
            return Err(Error::InvalidStatement);
        }
        context.require_current()?;
        let id = hash(&(peer_identity, &message.message_id).encode());
        let digest = hash(&payload);
        let invitation = Invitation {
            id,
            peer: peer_identity,
            root_key: resolved.chat_public_key,
            username: resolved.username,
            device_account: signer,
            device_key: message.content.device_enc_pub_key,
            message_id: message.message_id.clone(),
            timestamp: message.timestamp,
            text: message.content.welcome_text.clone().unwrap_or_default(),
        };
        self.validate_remote_device(&PeerDevice {
            account_id: signer,
            public_key: invitation.device_key,
        })?;
        let plaintext =
            wire::encode_chat_request_v2(&request).map_err(|_| Error::InvalidStatement)?;
        let valid = context.session_valid.clone();
        self.store
            .update(move |state| {
                if !valid() {
                    return Err(Error::NotConnected);
                }
                state.expire_receipts(now);
                let replay_id = format!("invite:{}", invitation.message_id);
                if let Some(previous) = state
                    .received
                    .iter()
                    .find(|entry| entry.peer == peer_identity && entry.request_id == replay_id)
                {
                    return if previous.digest == digest {
                        Ok(())
                    } else {
                        Err(Error::InvalidStatement)
                    };
                }
                if state.invitations.len() >= 16 || state.received.len() >= MAX_RECEIPTS {
                    return Err(Error::StorageUnavailable);
                }
                if let Some(peer) = state
                    .peers
                    .iter()
                    .find(|peer| peer.identity == peer_identity)
                {
                    if peer.root_key != invitation.root_key {
                        return Err(Error::InvalidStatement);
                    }
                } else {
                    if state.peers.len() >= MAX_PEERS {
                        return Err(Error::StorageUnavailable);
                    }
                    state.peers.push(Peer::new(
                        peer_identity,
                        invitation.root_key,
                        invitation.username.clone(),
                    ));
                }
                state.received.push(Receipt {
                    peer: peer_identity,
                    request_id: replay_id,
                    digest,
                    timestamp: now,
                });
                // Only authentication evidence is retained. The product owns the
                // visible invitation and decides whether to prepare an acceptance.
                let mut invitation = invitation;
                invitation.text.clear();
                state.invitations.push(invitation);
                Ok(())
            })
            .await?;
        Ok(HostNativeChatOpened {
            peer_identity,
            sender_account_id: signer,
            route: HostNativeChatRoute::Invitation,
            plaintext,
        })
    }

    pub(super) fn authorize_prepare(
        &self,
        state: &mut State,
        peer: &mut Peer,
        route: HostNativeChatRoute,
        plaintext: &[u8],
    ) -> Result<bool, Error> {
        let exchange = open_identity_exchange(plaintext).map_err(|_| Error::InvalidRequest)?;
        let (request_id, messages) = match exchange {
            OpenedDeviceExchange::Response { request_id, .. } => {
                let authenticated = state.boundary.incoming.iter().any(|entry| {
                    entry.peer == peer.identity
                        && entry.request_id == request_id
                        && entry.route == route
                });
                if !authenticated && (!peer.established || peer.active_devices().is_empty()) {
                    return Err(Error::PeerNotReady);
                }
                return Ok(true);
            }
            OpenedDeviceExchange::Request {
                request_id,
                messages,
            } => (request_id, messages),
        };
        if request_id.starts_with("pay-") {
            return Err(Error::InvalidRequest);
        }
        let mut added = false;
        let mut removed = false;
        let mut accepted = false;
        for message in messages {
            match message {
                OpenedDeviceMessage::DeviceControl(control) => {
                    if !valid_peer_timestamp(control.timestamp, current_unix_secs()) {
                        return Err(Error::InvalidRequest);
                    }
                    match control.content {
                        DeviceLifecycle::Added(device) => {
                            if device.account_id != self.public.account_id
                                || device.public_key != self.public.chat_public_key
                            {
                                return Err(Error::InvalidRequest);
                            }
                            added = true;
                        }
                        DeviceLifecycle::Removed(account) => {
                            if account != self.legacy_account {
                                return Err(Error::InvalidRequest);
                            }
                            removed = true;
                        }
                        DeviceLifecycle::MultiAccepted { request_id, device } => {
                            if route != HostNativeChatRoute::Identity
                                || device.account_id != self.public.account_id
                                || device.public_key != self.public.chat_public_key
                            {
                                return Err(Error::InvalidRequest);
                            }
                            let invitation = state
                                .invitations
                                .iter()
                                .find(|invite| {
                                    invite.peer == peer.identity && invite.message_id == request_id
                                })
                                .ok_or(Error::InvalidRequest)?;
                            if invitation.root_key != peer.root_key {
                                return Err(Error::InvalidStatement);
                            }
                            admit_device(
                                peer,
                                PeerDevice {
                                    account_id: invitation.device_account,
                                    public_key: invitation.device_key,
                                },
                                invitation.timestamp,
                                &invitation.message_id,
                            )?;
                            peer.established = true;
                            accepted = true;
                        }
                        DeviceLifecycle::LeftChat => {
                            if !peer.established {
                                return Err(Error::PeerNotReady);
                            }
                            peer.revocation_acked = false;
                            peer.revocation_acks.clear();
                            peer.revocation_request = None;
                        }
                        DeviceLifecycle::ContactAdded => {
                            if !peer.established {
                                return Err(Error::PeerNotReady);
                            }
                        }
                        DeviceLifecycle::Accepted { .. } => return Err(Error::InvalidRequest),
                    }
                }
                OpenedDeviceMessage::Ordinary(_) if route == HostNativeChatRoute::Device => {}
                OpenedDeviceMessage::PushToken { timestamp, .. }
                    if valid_peer_timestamp(timestamp, current_unix_secs()) => {}
                // Outgoing main-purse memos and private file/history capabilities
                // have dedicated trusted preparation. Guest bytes never inject them.
                _ => return Err(Error::InvalidRequest),
            }
        }
        if !peer.established || peer.active_devices().is_empty() {
            return Err(Error::PeerNotReady);
        }
        if route == HostNativeChatRoute::Identity && !accepted {
            return Err(Error::InvalidRequest);
        }
        if added || removed {
            if route != HostNativeChatRoute::Device || !added || !removed {
                return Err(Error::InvalidRequest);
            }
            if peer.revocation_request.as_ref() != Some(&request_id) {
                peer.revocation_request = Some(request_id);
                peer.revocation_acked = false;
                peer.revocation_acks.clear();
            }
        }
        Ok(false)
    }

    async fn receive_acknowledgment(
        self: &Arc<Self>,
        context: &NativeChatContext,
        registry: &NativeChatRegistry,
        identity: [u8; 32],
        sender: [u8; 32],
        request_id: String,
        response_code: u8,
    ) -> Result<(), Error> {
        let valid = context.session_valid.clone();
        let actor = self.clone();
        self.store
            .update(move |state| {
                if !valid() {
                    return Err(Error::NotConnected);
                }
                let peer = state.peer(&identity)?;
                if !peer
                    .devices
                    .iter()
                    .any(|device| device.active && device.account == sender)
                {
                    return Err(Error::InvalidStatement);
                }
                if let Some(entry) = state.outbox.iter().find(|entry| {
                    entry.peer == identity
                        && entry.request_id == request_id
                        && matches!(entry.kind, OutgoingKind::Payment(_))
                }) {
                    // Only a recipient of the committed envelope can acknowledge
                    // custody. Current roster membership alone is insufficient.
                    let shared = actor
                        .device
                        .identity_shared_secret(&peer.root_key)
                        .map_err(|_| Error::InvalidStatement)?;
                    let plaintext = native_root_open(
                        &shared,
                        entry
                            .statement
                            .data
                            .as_deref()
                            .ok_or(Error::StorageUnavailable)?,
                    )
                    .map_err(|_| Error::StorageUnavailable)?;
                    let wire::V2StatementTransportData::MultiRequest(request) =
                        wire::decode_transport_plaintext(&plaintext)
                            .map_err(|_| Error::StorageUnavailable)?
                    else {
                        return Err(Error::StorageUnavailable);
                    };
                    if !request
                        .devices_info
                        .iter()
                        .any(|device| device.statement_account_id == sender)
                    {
                        return Err(Error::InvalidStatement);
                    }
                }
                let peer = state.peer_mut(&identity)?;
                if response_code == 0 && peer.revocation_request.as_deref() == Some(&request_id) {
                    if !peer.revocation_acks.contains(&sender) {
                        peer.revocation_acks.push(sender);
                    }
                    peer.revocation_acked = peer
                        .devices
                        .iter()
                        .filter(|device| device.active)
                        .all(|device| peer.revocation_acks.contains(&device.account));
                }
                let revocation_ready = peer.revocation_acked;
                if let Some(entry) = state
                    .outbox
                    .iter()
                    .find(|entry| entry.peer == identity && entry.request_id == request_id)
                    && response_code == 0
                {
                    if let OutgoingKind::Payment(id) = entry.kind
                        && !state.payment_acknowledgments.contains(&id)
                    {
                        if state.payment_acknowledgments.len() >= MAX_RECEIPTS {
                            return Err(Error::StorageUnavailable);
                        }
                        state.payment_acknowledgments.push(id);
                    }
                    if entry.kind != OutgoingKind::Revocation || revocation_ready {
                        state.outbox.retain(|entry| {
                            !(entry.peer == identity && entry.request_id == request_id)
                        });
                    }
                }
                Ok(())
            })
            .await?;
        self.replay_payment_acknowledgments(context, registry).await
    }

    pub(super) async fn replay_payment_acknowledgments(
        &self,
        context: &NativeChatContext,
        registry: &NativeChatRegistry,
    ) -> Result<(), Error> {
        let pending = self
            .store
            .read(|state| state.payment_acknowledgments.clone())
            .await?;
        if pending.is_empty() {
            return Ok(());
        }
        let wallet = registry
            .existing_wallet(context, true)
            .await?
            .ok_or(Error::StorageUnavailable)?;
        for id in pending {
            wallet.note_delivery(context, &self.product, id).await?;
            let valid = context.session_valid.clone();
            self.store
                .update(move |state| {
                    if !valid() {
                        return Err(Error::NotConnected);
                    }
                    state
                        .payment_acknowledgments
                        .retain(|pending| *pending != id);
                    Ok(())
                })
                .await?;
        }
        Ok(())
    }
}

// Native OutgoingRequestQueue preserves message timestamps while the channel
// refreshes the signed statement's expiry. Body age is ordering metadata, not a
// second transport TTL: delayed admitted-peer traffic remains valid. Discovery
// invitations keep the separate bounded `fresh` policy.
pub(super) fn valid_peer_timestamp(timestamp_ms: u64, now: u64) -> bool {
    let timestamp = timestamp_ms / 1000;
    timestamp >= wire::PROTOCOL_EPOCH_SECONDS && timestamp <= now.saturating_add(CLOCK_SKEW)
}

fn admit_device(
    peer: &mut Peer,
    device: PeerDevice,
    timestamp: u64,
    message_id: &str,
) -> Result<(), Error> {
    if let Some(record) = peer
        .devices
        .iter_mut()
        .find(|record| record.account == device.account_id)
    {
        if record.key.is_some_and(|key| key != device.public_key) {
            return Err(Error::InvalidStatement);
        }
        if (timestamp, message_id) <= (record.timestamp, record.message_id.as_str()) {
            return Ok(());
        }
        let was_active = record.active;
        record.key = Some(device.public_key);
        record.active = true;
        record.timestamp = timestamp;
        record.message_id = message_id.to_owned();
        if !was_active {
            peer.revision = peer
                .revision
                .checked_add(1)
                .ok_or(Error::StorageUnavailable)?;
        }
    } else {
        if peer.devices.len() >= 64 {
            return Err(Error::StorageUnavailable);
        }
        peer.devices.push(DeviceRecord {
            account: device.account_id,
            key: Some(device.public_key),
            active: true,
            timestamp,
            message_id: message_id.to_owned(),
        });
        peer.revision = peer
            .revision
            .checked_add(1)
            .ok_or(Error::StorageUnavailable)?;
    }
    if peer.active_devices().len() > 16 {
        return Err(Error::StorageUnavailable);
    }
    Ok(())
}

fn apply_control(
    peer: &mut Peer,
    control: DeviceControl,
    sender: &PeerDevice,
    admitted_sender: bool,
    last_departure: Option<u64>,
) -> Result<(), Error> {
    match control.content {
        DeviceLifecycle::Added(device) => {
            admit_device(peer, device, control.timestamp, &control.message_id)?
        }
        DeviceLifecycle::Removed(account) => {
            if let Some(record) = peer
                .devices
                .iter_mut()
                .find(|record| record.account == account)
            {
                if (control.timestamp, control.message_id.as_str())
                    > (record.timestamp, record.message_id.as_str())
                {
                    let changed = record.active;
                    record.active = false;
                    record.timestamp = control.timestamp;
                    record.message_id = control.message_id;
                    if changed {
                        peer.revision = peer
                            .revision
                            .checked_add(1)
                            .ok_or(Error::StorageUnavailable)?;
                    }
                }
            } else {
                if peer.devices.len() >= 64 {
                    return Err(Error::StorageUnavailable);
                }
                peer.devices.push(DeviceRecord {
                    account,
                    key: None,
                    active: false,
                    timestamp: control.timestamp,
                    message_id: control.message_id,
                });
            }
        }
        DeviceLifecycle::LeftChat => {
            let mut changed = false;
            for record in &mut peer.devices {
                if (control.timestamp, control.message_id.as_str())
                    > (record.timestamp, record.message_id.as_str())
                {
                    changed |= record.active;
                    record.active = false;
                    record.timestamp = control.timestamp;
                    record.message_id = control.message_id.clone();
                }
            }
            // Delayed departure must not undo a newer authenticated device
            // admission. Per-device tombstones still advance monotonically.
            if changed {
                if !peer.devices.iter().any(|record| record.active) {
                    peer.established = false;
                }
                peer.revocation_acked = false;
                peer.revision = peer
                    .revision
                    .checked_add(1)
                    .ok_or(Error::StorageUnavailable)?;
            }
        }
        DeviceLifecycle::ContactAdded => {
            // Native compatibility signal for an older outgoing request, not a
            // device advertisement. It may only resolve a request using an
            // independently admitted signer, never create or revive a binding.
            if admitted_sender
                && peer.invitation.is_some()
                && peer
                    .invitation_timestamp
                    .is_some_and(|timestamp| control.timestamp <= timestamp)
                && last_departure.is_none_or(|timestamp| timestamp < control.timestamp)
            {
                peer.invitation = None;
                peer.invitation_timestamp = None;
                peer.established = true;
            }
        }
        DeviceLifecycle::MultiAccepted { request_id, device } => {
            if device != *sender {
                return Err(Error::InvalidStatement);
            }
            if peer.invitation.as_deref() == Some(&request_id) {
                admit_device(peer, device, control.timestamp, &control.message_id)?;
                if !peer.devices.iter().any(|record| {
                    record.active
                        && record.account == device.account_id
                        && record.key == Some(device.public_key)
                }) {
                    return Err(Error::InvalidStatement);
                }
                peer.established = true;
                peer.invitation = None;
                peer.invitation_timestamp = None;
            }
        }
        DeviceLifecycle::Accepted { request_id } => {
            // Legacy acceptance carries no key. Correlation alone cannot grant
            // roster authority, including in a batch with DeviceAdded.
            if !admitted_sender {
                return Err(Error::InvalidStatement);
            }
            if peer.invitation.as_deref() == Some(&request_id) {
                peer.established = true;
                peer.invitation = None;
                peer.invitation_timestamp = None;
            }
        }
    }
    Ok(())
}

fn exchange_digest(messages: &[OpenedDeviceMessage]) -> Result<[u8; 32], Error> {
    let mut hasher = blake2b_simd::Params::new().hash_length(32).to_state();
    hasher.update(&(messages.len() as u32).to_le_bytes());
    for message in messages {
        let encoded = match message {
            OpenedDeviceMessage::Ordinary(bytes) => {
                hasher.update(&[0]);
                hasher.update(&(bytes.len() as u32).to_le_bytes());
                hasher.update(bytes);
                continue;
            }
            OpenedDeviceMessage::Payment(memo) => {
                hasher.update(&[1]);
                let mut bytes = Zeroizing::new(
                    (memo.message_id.as_str(), memo.timestamp, memo.total_value).encode(),
                );
                parity_scale_codec::Compact(memo.coin_keys.len() as u32).encode_to(&mut *bytes);
                for key in memo.coin_keys.iter() {
                    key.encode_to(&mut *bytes);
                }
                hasher.update(&(bytes.len() as u32).to_le_bytes());
                hasher.update(&bytes);
                continue;
            }
            OpenedDeviceMessage::CompactedHistory(reference) => {
                hasher.update(&[3]);
                hasher.update(&history::reference_digest(reference));
                continue;
            }
            OpenedDeviceMessage::RichContent(message) => {
                hasher.update(&[4]);
                hasher.update(&message.digest);
                continue;
            }
            OpenedDeviceMessage::PushToken { digest, .. } => {
                hasher.update(&[5]);
                hasher.update(digest);
                continue;
            }
            OpenedDeviceMessage::ProfileReference(frame) => {
                hasher.update(&[6]);
                let bytes = (
                    frame.message_id.as_str(),
                    frame.timestamp,
                    frame.discloser_product_id.as_str(),
                    frame.reference.as_deref(),
                )
                    .encode();
                hasher.update(&(bytes.len() as u32).to_le_bytes());
                hasher.update(&bytes);
                continue;
            }
            OpenedDeviceMessage::DeviceControl(control) => {
                let mut bytes = (control.message_id.as_str(), control.timestamp).encode();
                match &control.content {
                    DeviceLifecycle::Added(device) => {
                        (0u8, device.account_id, device.public_key).encode_to(&mut bytes)
                    }
                    DeviceLifecycle::Removed(account) => (1u8, account).encode_to(&mut bytes),
                    DeviceLifecycle::Accepted { request_id } => {
                        (2u8, request_id).encode_to(&mut bytes)
                    }
                    DeviceLifecycle::MultiAccepted { request_id, device } => {
                        (3u8, request_id, device.account_id, device.public_key)
                            .encode_to(&mut bytes)
                    }
                    DeviceLifecycle::ContactAdded => bytes.push(4),
                    DeviceLifecycle::LeftChat => bytes.push(5),
                }
                bytes
            }
        };
        hasher.update(&[2]);
        hasher.update(&(encoded.len() as u32).to_le_bytes());
        hasher.update(&encoded);
    }
    Ok(hasher
        .finalize()
        .as_bytes()
        .try_into()
        .expect("32-byte digest"))
}
