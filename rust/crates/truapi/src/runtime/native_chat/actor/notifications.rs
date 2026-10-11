// SPDX-License-Identifier: AGPL-3.0-only
//! Optional authenticated wakes. Native Chat ciphertext and delivery are unchanged.

use super::*;
use crate::runtime::notification_envelope::{
    NotificationHeader, encode_notification_envelope, signing_bytes,
};
use sha2::{Digest, Sha256};
use truapi::latest::HostNativeChatWake;

const WAKE_LIFETIME_MS: u64 = 86_400_000;
const WATCH_LIFETIME_MS: u64 = 7 * WAKE_LIFETIME_MS;

/// Never wake for ACKs, lifecycle/profile frames, reactions, edits or invitations.
pub fn notification_message(
    route: HostNativeChatRoute,
    plaintext: &[u8],
) -> Option<String> {
    if route != HostNativeChatRoute::Device {
        return None;
    }
    use crate::runtime::chat_device::{OpenedDeviceExchange, OpenedDeviceMessage, open_identity_exchange};
    let OpenedDeviceExchange::Request { request_id, messages } =
        open_identity_exchange(plaintext).ok()?
    else {
        return None;
    };
    messages.iter().any(|message| {
        let OpenedDeviceMessage::Ordinary(bytes) = message else { return false; };
        wire::decode_message(bytes).is_ok_and(|message| matches!(
            message.content,
            wire::V2ChatMessageContent::Text(_)
                | wire::V2ChatMessageContent::RichText { .. }
                | wire::V2ChatMessageContent::Reply { .. }
        ))
    }).then_some(request_id)
}

fn wake_channel(session: &[u8; 32]) -> [u8; 32] {
    hash(&("truapi:chat:notification-channel:v2", session).encode())
}

fn wake_topic(session: &[u8; 32]) -> [u8; 32] {
    hash(&("truapi:chat:notification-topic:v2", session).encode())
}

fn event_id(sender: &[u8; 32], request: &str) -> String {
    hex::encode(hash(&("truapi:chat:notification-event:v2", sender, request).encode()))
}

impl NativeChatActor {
    pub async fn retained_notification(
        self: &Arc<Self>,
        context: &NativeChatContext,
        peer: [u8; 32],
        request_id: &str,
    ) -> Result<Option<HostNativeChatWake>, Error> {
        self.require_migrated().await?;
        context.require_current()?;
        let witness = self.store.read(|state| {
            state.outbox.iter().find(|entry| {
                entry.peer == peer && entry.request_id == request_id && match &entry.kind {
                    OutgoingKind::Payment(_) => true,
                    OutgoingKind::Rich(key) => files::notification_candidate(state, key),
                    _ => false,
                }
            }).and_then(|entry| entry.statement.data.as_deref().map(hash))
        }).await?;
        let Some(witness) = witness else { return Ok(None); };
        self.sign_notification(context, peer, request_id, witness).await
    }

    pub async fn notification_wake(
        self: &Arc<Self>,
        context: &NativeChatContext,
        prepared: &HostNativeChatPrepared,
    ) -> Result<Option<HostNativeChatWake>, Error> {
        context.require_current()?;
        if !prepared.requires_ack {
            return Ok(None);
        }
        let witness = hash(prepared.statement.data.as_deref().ok_or(Error::InvalidStatement)?);
        self.sign_notification(context, prepared.peer_identity, &prepared.request_id, witness).await
    }

    async fn sign_notification(
        self: &Arc<Self>,
        context: &NativeChatContext,
        peer_identity: [u8; 32],
        request_id: &str,
        witness: [u8; 32],
    ) -> Result<Option<HostNativeChatWake>, Error> {
        context.require_current()?;
        let peer = self.store.read(|state| state.peer(&peer_identity).cloned()).await??;
        if !peer.ready() {
            return Ok(None);
        }
        let shared = self.device.identity_shared_secret(&peer.root_key)
            .map_err(|_| Error::InvalidStatement)?;
        let session = chat_identity_session_id(&shared, &self.public.account_id, &peer.identity);
        let channel = wake_channel(&session);
        let topic = wake_topic(&session);
        // A fixed-size witness binds the carrier without copying its ciphertext.
        let created_at = current_unix_secs().saturating_mul(1000);
        let mut header = NotificationHeader {
            v: 2,
            product: self.product.clone(),
            genesis: hex::encode(context.genesis_hash),
            channel: hex::encode(channel),
            topics: vec![hex::encode(topic)],
            event_id: event_id(&self.public.account_id, request_id),
            created_at,
            expires_at: created_at.saturating_add(WAKE_LIFETIME_MS),
            ciphertext_digest: hex::encode(Sha256::digest(witness)),
            sender_key: hex::encode(self.public.account_id),
            signature: String::new(),
        };
        header.signature = hex::encode(self.signer.secret.sign_simple(
            SR25519_SIGNING_CONTEXT,
            &signing_bytes(&header).map_err(|_| Error::InvalidRequest)?,
            &self.signer.public,
        ).to_bytes());
        let data = encode_notification_envelope(&header, &witness)
            .map_err(|_| Error::InvalidRequest)?;
        let actor = self.clone();
        let valid = context.session_valid.clone();
        let peer_identity = peer.identity;
        let statement = self.store.update(move |state| {
            if !valid() {
                return Err(Error::NotConnected);
            }
            if !state.peer(&peer_identity)?.ready() {
                return Err(Error::PeerNotReady);
            }
            actor.sign(state, channel, vec![topic], data)
        }).await?;
        context.require_current()?;
        Ok(Some(HostNativeChatWake {
            peer_identity,
            request_id: request_id.to_owned(),
            statement,
        }))
    }

    pub async fn notification_watches(
        &self,
        context: &NativeChatContext,
    ) -> Result<Vec<ReceivingWatch>, Error> {
        self.require_migrated().await?;
        context.require_current()?;
        let expires_at = current_unix_secs().saturating_mul(1000).saturating_add(WATCH_LIFETIME_MS);
        let watches = self.store.read(|state| {
            let mut watches = Vec::new();
            for peer in state.peers.iter().filter(|peer| peer.ready()) {
                for device in peer.active_devices() {
                    if watches.len() == 256 {
                        return Err(Error::InvalidRequest);
                    }
                    let shared = chat_shared_secret(&self.root_secret, &device.public_key)
                        .map_err(|_| Error::InvalidStatement)?;
                    let session = chat_identity_session_id(&shared, &device.account_id, &self.public.identity_account_id);
                    watches.push(ReceivingWatch {
                        id: format!("{}{}", hex::encode(peer.identity), hex::encode(device.account_id)),
                        genesis: hex::encode(context.genesis_hash),
                        channel: hex::encode(wake_channel(&session)),
                        topics: vec![hex::encode(wake_topic(&session))],
                        senders: vec![format!("sr25519:{}", hex::encode(device.account_id))],
                        expires_at,
                        muted_until: 0,
                        route: format!("/peer/{}", hex::encode(peer.identity)),
                    });
                }
            }
            Ok(watches)
        }).await??;
        context.require_current()?;
        Ok(watches)
    }
}
