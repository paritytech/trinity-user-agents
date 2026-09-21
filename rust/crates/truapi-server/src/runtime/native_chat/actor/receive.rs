// SPDX-License-Identifier: AGPL-3.0-only
//! Authenticate first, persist private claims and roster state, acknowledge last.

use super::*;
use crate::host_logic::statement_store::{
    decode_verified_statement_data, statement_expiry_elapsed,
};
use crate::runtime::chat_device::{
    DeviceControl, DeviceLifecycle, OpenedDeviceExchange, OpenedDeviceMessage,
    open_identity_exchange,
};
use schnorrkel::{PublicKey, Signature};

impl NativeChatActor {
    pub(in crate::runtime::native_chat) async fn receive(
        self: &Arc<Self>,
        context: &NativeChatContext,
        registry: &NativeChatRegistry,
        statement: SignedStatement,
    ) -> Result<(), Error> {
        // This gate is never acquired by outgoing payment handoff. No store
        // mutation lock is held while awaiting the wallet's claim/settlement gate.
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
        let now = current_unix_secs();
        if verified
            .expiry
            .is_none_or(|expiry| statement_expiry_elapsed(expiry, now))
        {
            return Err(Error::InvalidStatement);
        }
        if statement.topics.contains(&wire::chat_request_full_topic(
            &self.public.identity_account_id,
        )) {
            return self
                .receive_invitation(context, statement, verified.signer, verified.data)
                .await;
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
            if statement.topics.contains(&root_incoming)
                && wire::chat_identity_request_topic(&root_incoming).ok() == Some(channel)
            {
                let plaintext = native_root_open(&root_shared, &verified.data)
                    .map_err(|_| Error::InvalidStatement)?;
                let exchange =
                    open_identity_exchange(&plaintext).map_err(|_| Error::InvalidStatement)?;
                return self
                    .receive_acceptance(context, registry, &peer, verified.signer, exchange)
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
            if statement.topics.contains(&root_incoming)
                && wire::chat_identity_response_topic(&root_incoming).ok() == Some(channel)
            {
                let plaintext = native_root_open(&root_shared, &verified.data)
                    .map_err(|_| Error::InvalidStatement)?;
                return match open_identity_exchange(&plaintext)
                    .map_err(|_| Error::InvalidStatement)?
                {
                    OpenedDeviceExchange::Response {
                        request_id,
                        response_code,
                    } => {
                        self.receive_acknowledgment(
                            context,
                            registry,
                            peer.identity,
                            sender.account_id,
                            request_id,
                            response_code,
                        )
                        .await
                    }
                    _ => Err(Error::InvalidStatement),
                };
            }
            let incoming_shared = chat_shared_secret(&self.root_secret, &sender.public_key)
                .map_err(|_| Error::InvalidStatement)?;
            let incoming_session = chat_identity_session_id(
                &incoming_shared,
                &sender.account_id,
                &self.public.identity_account_id,
            );
            let request_route = statement.topics.contains(&incoming_session)
                && wire::chat_identity_request_topic(&incoming_session).ok() == Some(channel);
            let response_route = statement.topics.contains(&incoming_session)
                && wire::chat_identity_response_topic(&incoming_session).ok() == Some(channel);
            if !request_route && !response_route {
                continue;
            }
            let plaintext = native_root_open(&incoming_shared, &verified.data)
                .map_err(|_| Error::InvalidStatement)?;
            let exchange = self
                .device
                .open_multi_device(&sender, &plaintext)
                .map_err(|_| Error::InvalidStatement)?;
            return match exchange {
                OpenedDeviceExchange::Request {
                    request_id,
                    messages,
                } if request_route => {
                    self.receive_messages(
                        context, registry, peer, sender, request_id, messages, false,
                    )
                    .await
                }
                OpenedDeviceExchange::Response {
                    request_id,
                    response_code,
                } if response_route => {
                    self.receive_acknowledgment(
                        context,
                        registry,
                        peer.identity,
                        sender.account_id,
                        request_id,
                        response_code,
                    )
                    .await
                }
                _ => Err(Error::InvalidStatement),
            };
        }
        Err(Error::InvalidStatement)
    }

    async fn receive_invitation(
        self: &Arc<Self>,
        context: &NativeChatContext,
        statement: SignedStatement,
        signer: [u8; 32],
        data: Vec<u8>,
    ) -> Result<(), Error> {
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
        let valid = context.session_valid.clone();
        let auto_accept = self
            .store
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
                        Ok(false)
                    } else {
                        Err(Error::InvalidStatement)
                    };
                }
                if state.invitations.len() >= 16 || state.received.len() >= MAX_RECEIPTS {
                    return Err(Error::StorageUnavailable);
                }
                let auto_accept = state.peers.iter().any(|peer| {
                    peer.identity == peer_identity
                        && peer.established
                        && peer.root_key == invitation.root_key
                });
                state.received.push(Receipt {
                    peer: peer_identity,
                    request_id: replay_id,
                    digest,
                    timestamp: now,
                });
                state.invitations.push(invitation);
                Ok(auto_accept)
            })
            .await?;
        if auto_accept {
            self.accept_invitation(context, id).await?;
        }
        Ok(())
    }

    pub(in crate::runtime::native_chat) async fn accept(
        self: &Arc<Self>,
        context: &NativeChatContext,
        invitation_id: [u8; 32],
    ) -> Result<(), Error> {
        let _incoming = self.receiving.lock().await;
        self.accept_invitation(context, invitation_id).await
    }

    async fn accept_invitation(
        self: &Arc<Self>,
        context: &NativeChatContext,
        invitation_id: [u8; 32],
    ) -> Result<(), Error> {
        let actor = self.clone();
        let valid = context.session_valid.clone();
        let delivery = self.delivery_gate.lock().await;
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
                let invitation = state.invitations[position].clone();
                if !fresh(invitation.timestamp, current_unix_secs()) {
                    return Err(Error::InvalidStatement);
                }
                if let Some(peer) = state
                    .peers
                    .iter()
                    .find(|peer| peer.identity == invitation.peer)
                {
                    if peer.root_key != invitation.root_key {
                        return Err(Error::InvalidStatement);
                    }
                } else {
                    if state.peers.len() >= MAX_PEERS {
                        return Err(Error::StorageUnavailable);
                    }
                    state.peers.push(Peer {
                        identity: invitation.peer,
                        root_key: invitation.root_key,
                        username: invitation.username.clone(),
                        devices: Vec::new(),
                        invitation: None,
                        invitation_timestamp: None,
                        invitation_text: None,
                        established: false,
                        revocation_request: None,
                        revocation_acked: false,
                        revocation_acks: Vec::new(),
                        revision: 0,
                    });
                }
                {
                    let peer = state.peer_mut(&invitation.peer)?;
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
                    if invitation.username.is_some() {
                        peer.username = invitation.username.clone();
                    }
                }
                let peer = state.peer(&invitation.peer)?.clone();
                let now = current_unix_secs().saturating_mul(1000);
                let accepted = wire::encode_multi_chat_accepted_message(
                    &random_id()?,
                    now,
                    &invitation.message_id,
                    &wire::V2PeerDevice {
                        statement_account_id: actor.public.account_id,
                        encryption_public_key: actor.public.chat_public_key,
                    },
                )
                .map_err(|_| Error::InvalidRequest)?;
                let request_id = random_id()?;
                let plaintext = Zeroizing::new(
                    wire::encode_transport_request_plaintext(&request_id, &[accepted])
                        .map_err(|_| Error::InvalidRequest)?,
                );
                let shared = chat_shared_secret(&actor.root_secret, &peer.root_key)
                    .map_err(|_| Error::InvalidStatement)?;
                let topic = chat_identity_session_id(
                    &shared,
                    &actor.public.identity_account_id,
                    &peer.identity,
                );
                let channel = wire::chat_identity_request_topic(&topic)
                    .map_err(|_| Error::InvalidStatement)?;
                let encrypted =
                    native_root_seal(&shared, &plaintext).map_err(|_| Error::InvalidStatement)?;
                let statement = actor.sign(state, channel, vec![topic], encrypted)?;
                state.queue(Outgoing {
                    peer: peer.identity,
                    request_id,
                    digest: hash(&plaintext),
                    kind: OutgoingKind::Acceptance,
                    roster_revision: peer.revision,
                    statement,
                    last_attempt: 0,
                })?;
                actor.queue_revocation(state, &peer.identity)?;
                state.invitations.remove(position);
                if !invitation.text.is_empty() {
                    state.record_messages(HostNativeChatMessages {
                        peer_identity: peer.identity,
                        incoming: true,
                        request_id: invitation.message_id.clone(),
                        messages: vec![
                            wire::encode_rich_text_message(
                                &invitation.message_id,
                                invitation.timestamp,
                                Some(&invitation.text),
                                None,
                            )
                            .map_err(|_| Error::InvalidStatement)?,
                        ],
                    });
                }
                Ok(())
            })
            .await?;
        drop(delivery);
        self.start_delivery(context);
        self.flush(context).await
    }

    async fn receive_acceptance(
        self: &Arc<Self>,
        context: &NativeChatContext,
        registry: &NativeChatRegistry,
        peer: &Peer,
        signer: [u8; 32],
        exchange: OpenedDeviceExchange,
    ) -> Result<(), Error> {
        let OpenedDeviceExchange::Request {
            request_id,
            messages,
        } = exchange
        else {
            return Err(Error::InvalidStatement);
        };
        // Root encryption authenticates the peer identity, not an arbitrary
        // statement signer. An unadmitted device must bind itself to our pending
        // invitation; neither ContactAdded nor DeviceAdded can authorize it.
        let sender = peer
            .devices
            .iter()
            .find(|device| device.active && device.account == signer)
            .and_then(|device| {
                device.key.map(|public_key| PeerDevice {
                    account_id: signer,
                    public_key,
                })
            })
            .or_else(|| {
                messages.iter().find_map(|message| match message {
                    OpenedDeviceMessage::DeviceControl(DeviceControl {
                        content: DeviceLifecycle::MultiAccepted { request_id, device },
                        ..
                    }) if peer.invitation.as_deref() == Some(request_id)
                        && device.account_id == signer =>
                    {
                        Some(*device)
                    }
                    _ => None,
                })
            })
            .ok_or(Error::InvalidStatement)?;
        self.receive_messages(
            context,
            registry,
            peer.clone(),
            sender,
            request_id,
            messages,
            true,
        )
        .await
    }

    fn queue_revocation(&self, state: &mut State, identity: &[u8; 32]) -> Result<(), Error> {
        let peer = state.peer(identity)?.clone();
        let devices = peer.active_devices();
        if devices.is_empty() {
            return Ok(());
        }
        let request_id = random_id()?;
        let now = current_unix_secs().saturating_mul(1000);
        let added = wire::encode_device_added_message(
            &random_id()?,
            now,
            &self.public.account_id,
            &self.public.chat_public_key,
        )
        .map_err(|_| Error::InvalidRequest)?;
        let removed = wire::encode_device_removed_message(&random_id()?, now, &self.legacy_account)
            .map_err(|_| Error::InvalidRequest)?;
        let messages = vec![added, removed];
        let digest = hash(&messages.encode());
        let statement = self.multi_statement(state, &peer, &devices, &request_id, &messages)?;
        state.outbox.retain(|entry| {
            !(entry.peer == peer.identity && entry.kind == OutgoingKind::Revocation)
        });
        let current = state.peer_mut(identity)?;
        current.revocation_request = Some(request_id.clone());
        current.revocation_acked = false;
        current.revocation_acks.clear();
        state.queue(Outgoing {
            peer: peer.identity,
            request_id,
            digest,
            kind: OutgoingKind::Revocation,
            roster_revision: peer.revision,
            statement,
            last_attempt: 0,
        })
    }

    fn queue_identity_ack(
        &self,
        state: &mut State,
        peer: &Peer,
        request_id: &str,
    ) -> Result<(), Error> {
        let shared = chat_shared_secret(&self.root_secret, &peer.root_key)
            .map_err(|_| Error::InvalidStatement)?;
        let session =
            chat_identity_session_id(&shared, &self.public.identity_account_id, &peer.identity);
        let plaintext = Zeroizing::new(
            wire::encode_transport_response_plaintext(request_id, 0)
                .map_err(|_| Error::InvalidRequest)?,
        );
        let encrypted =
            native_root_seal(&shared, &plaintext).map_err(|_| Error::InvalidStatement)?;
        let channel =
            wire::chat_identity_response_topic(&session).map_err(|_| Error::InvalidStatement)?;
        let statement = self.sign(state, channel, vec![session], encrypted)?;
        state.queue(Outgoing {
            peer: peer.identity,
            request_id: request_id.to_owned(),
            digest: hash(&plaintext),
            kind: OutgoingKind::Acknowledgment,
            roster_revision: peer.revision,
            statement,
            last_attempt: 0,
        })
    }

    fn queue_device_ack(
        &self,
        state: &mut State,
        peer: &Peer,
        sender: &PeerDevice,
        request_id: &str,
    ) -> Result<(), Error> {
        let plaintext = Zeroizing::new(
            wire::encode_transport_response_plaintext(request_id, 0)
                .map_err(|_| Error::InvalidRequest)?,
        );
        let inner = Zeroizing::new(
            self.device
                .seal_multi_device(&[*sender], &plaintext)
                .map_err(|_| Error::InvalidStatement)?,
        );
        let shared = self
            .device
            .identity_shared_secret(&peer.root_key)
            .map_err(|_| Error::InvalidStatement)?;
        let session = chat_identity_session_id(&shared, &self.public.account_id, &peer.identity);
        let encrypted = native_root_seal(&shared, &inner).map_err(|_| Error::InvalidStatement)?;
        let channel =
            wire::chat_identity_response_topic(&session).map_err(|_| Error::InvalidStatement)?;
        let statement = self.sign(state, channel, vec![session], encrypted)?;
        state.queue(Outgoing {
            peer: peer.identity,
            request_id: request_id.to_owned(),
            digest: hash(&plaintext),
            kind: OutgoingKind::Acknowledgment,
            roster_revision: peer.revision,
            statement,
            last_attempt: 0,
        })
    }

    async fn receive_messages(
        self: &Arc<Self>,
        context: &NativeChatContext,
        registry: &NativeChatRegistry,
        peer: Peer,
        sender: PeerDevice,
        request_id: String,
        messages: Vec<OpenedDeviceMessage>,
        identity_route: bool,
    ) -> Result<(), Error> {
        let digest = exchange_digest(&messages)?;
        let previous = self
            .store
            .read(|state| {
                state
                    .received
                    .iter()
                    .find(|entry| entry.peer == peer.identity && entry.request_id == request_id)
                    .cloned()
            })
            .await?;
        if previous
            .as_ref()
            .is_some_and(|previous| previous.digest != digest)
        {
            return Err(Error::InvalidStatement);
        }
        let mut controls = Vec::new();
        let mut content = Vec::new();
        if previous.is_none() {
            for message in messages {
                match message {
                    OpenedDeviceMessage::DeviceControl(control) => {
                        if !valid_peer_timestamp(control.timestamp, current_unix_secs()) {
                            return Err(Error::InvalidStatement);
                        }
                        controls.push(control);
                    }
                    other => content.push(other),
                }
            }
        }
        controls.sort_by(|left, right| {
            (left.timestamp, &left.message_id).cmp(&(right.timestamp, &right.message_id))
        });
        // Validate the entire control batch before accepting any spendable
        // material. The receive gate serializes every roster admission.
        let identity = peer.identity;
        let previous_revision = peer.revision;
        let previous_invitation = peer.invitation.clone();
        let admitted_sender = peer.devices.iter().any(|device| {
            device.active
                && device.account == sender.account_id
                && device.key == Some(sender.public_key)
        });
        let last_departure = controls
            .iter()
            .filter_map(|control| {
                matches!(&control.content, DeviceLifecycle::LeftChat).then_some(control.timestamp)
            })
            .max();
        let mut prospective = peer;
        for control in controls {
            apply_control(
                &mut prospective,
                control,
                &sender,
                admitted_sender,
                last_departure,
            )?;
        }
        let accepted_invitation = previous_invitation
            .as_ref()
            .filter(|_| prospective.invitation.is_none())
            .cloned();
        if !admitted_sender && accepted_invitation.is_none() {
            return Err(Error::InvalidStatement);
        }
        let history::ExpandedHistory {
            ordinary,
            payments,
            rich,
            imports,
        } = self.expand_history(context, identity, content).await?;
        let rich = self
            .prepare_rich(context, identity, &request_id, rich)
            .await?;
        if !payments.is_empty() {
            // The wallet preflights the complete batch before effects. Success
            // means every memo and claim plan is durable, not merely queued.
            registry
                .wallet(context)
                .await?
                .receive_batch(context, &self.product, identity, &request_id, payments)
                .await?;
        }
        let delivery = self.delivery_gate.lock().await;
        let actor = self.clone();
        let valid = context.session_valid.clone();
        self.store
            .update(move |state| {
                if !valid() {
                    return Err(Error::NotConnected);
                }
                let current = state.peer(&identity)?;
                if current.revision != previous_revision
                    || current.invitation != previous_invitation
                    || (admitted_sender
                        && !current.devices.iter().any(|device| {
                            device.active
                                && device.account == sender.account_id
                                && device.key == Some(sender.public_key)
                        }))
                {
                    return Err(Error::InvalidStatement);
                }
                let exists = state
                    .received
                    .iter()
                    .find(|entry| entry.peer == identity && entry.request_id == request_id);
                if let Some(existing) = exists {
                    if existing.digest != digest {
                        return Err(Error::InvalidStatement);
                    }
                } else {
                    state.expire_receipts(current_unix_secs());
                    if state.received.len() >= MAX_RECEIPTS {
                        return Err(Error::StorageUnavailable);
                    }
                    let changed_roster = prospective.revision != previous_revision;
                    *state.peer_mut(&identity)? = prospective;
                    if let Some(invitation_id) = accepted_invitation {
                        state.outbox.retain(|entry| {
                            !(entry.peer == identity && entry.kind == OutgoingKind::Invitation)
                        });
                        if state.acknowledgments.len() == MAX_HISTORY_BATCHES {
                            state.acknowledgments.remove(0);
                        }
                        state.acknowledgments.push(HostNativeChatAcknowledgment {
                            peer_identity: identity,
                            request_id: invitation_id,
                            response_code: 0,
                        });
                        actor.queue_revocation(state, &identity)?;
                    } else if changed_roster {
                        actor.queue_revocation(state, &identity)?;
                    }
                    state.record_messages(HostNativeChatMessages {
                        // The complete expansion is public only after every
                        // private payment memo and claim plan is durable.
                        peer_identity: identity,
                        incoming: true,
                        request_id: request_id.clone(),
                        messages: ordinary,
                    });
                    state.history_imports.extend(imports);
                    files::merge_received(state, rich)?;
                    state.received.push(Receipt {
                        peer: identity,
                        request_id: request_id.clone(),
                        digest,
                        timestamp: current_unix_secs(),
                    });
                }
                let current = state.peer(&identity)?.clone();
                if identity_route {
                    actor.queue_identity_ack(state, &current, &request_id)
                } else {
                    actor.queue_device_ack(state, &current, &sender, &request_id)
                }
            })
            .await?;
        drop(delivery);
        self.start_delivery(context);
        self.flush(context).await
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
                {
                    if response_code == 0 {
                        if let OutgoingKind::Payment(id) = entry.kind {
                            if !state.payment_acknowledgments.contains(&id) {
                                if state.payment_acknowledgments.len() >= MAX_RECEIPTS {
                                    return Err(Error::StorageUnavailable);
                                }
                                state.payment_acknowledgments.push(id);
                            }
                        }
                        if entry.kind != OutgoingKind::Revocation || revocation_ready {
                            state.outbox.retain(|entry| {
                                !(entry.peer == identity && entry.request_id == request_id)
                            });
                        }
                    }
                }
                let request_id = state
                    .sent
                    .iter()
                    .find(|receipt| {
                        receipt.peer == identity && receipt.wire_request_id == request_id
                    })
                    .map(|receipt| receipt.request_id.clone())
                    .unwrap_or(request_id);
                if !state.acknowledgments.iter().any(|ack| {
                    ack.peer_identity == identity
                        && ack.request_id == request_id
                        && ack.response_code == response_code
                }) {
                    if state.acknowledgments.len() == MAX_HISTORY_BATCHES {
                        state.acknowledgments.remove(0);
                    }
                    state.acknowledgments.push(HostNativeChatAcknowledgment {
                        peer_identity: identity,
                        request_id,
                        response_code,
                    });
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
        let wallet = registry.wallet(context).await?;
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
                hasher.update(&*bytes);
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
