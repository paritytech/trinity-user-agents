// SPDX-License-Identifier: AGPL-3.0-only
//! Host-originated profile references: the user's disclosed reference, sealed
//! to each established peer's devices and handed to the product as opaque
//! prepared statements, like payments and rich files.
//!
//! A per-peer watermark records what this Host last queued for that peer, so
//! the initial share, a new contact, a replacement and a withdrawal are one
//! reconcile: every peer whose watermark differs from the disclosure is sent
//! the disclosure. The watermark advances when the message is queued. Each
//! frame to a peer is timestamped later than the one before it, so the peer's
//! host keeps the newest whatever order it opens them in.
//!
//! Delivery is best effort. References have their own outbox budget, one per
//! peer, so they never take a slot user traffic needs; a reference that finds
//! no room is left for a later reconcile. A queued reference is offered for one
//! statement lifetime and then dropped rather than re-signed: a host that does
//! not know the content type never acknowledges it, and would otherwise hold
//! the slot for good.
//!
//! Known gap (docs/rfcs/profile-disclosure.md): advancing at queue time means a message that never
//! arrives is not resent until the disclosure changes.

use super::*;
use crate::runtime::native_chat::background::require_authorized;
use crate::runtime::profile::{Disclosure, ProfileOwner, read_disclosure};

/// What this Host last queued to one peer.
#[derive(Clone, PartialEq, Eq, Encode, Decode)]
pub(super) struct ProfileWatermark {
    pub(super) peer: [u8; 32],
    /// Digest of the disclosure sent, identifying it without keeping it;
    /// `None` once a withdrawal was sent.
    pub(super) digest: Option<[u8; 32]>,
    /// Product that disclosed it, repeated on a withdrawal.
    pub(super) discloser_product_id: String,
    /// Timestamp of the frame sent. The next frame to this peer is later.
    pub(super) timestamp: u64,
}

/// The wallet and Chat network the user's disclosure belongs to.
pub(super) fn profile_owner(context: &NativeChatContext) -> ProfileOwner {
    ProfileOwner {
        root_public_key: context.session.public_key,
        genesis_hash: context.genesis_hash,
    }
}

fn disclosure_digest(disclosure: &Disclosure) -> [u8; 32] {
    hash(
        &(
            b"native-chat-profile-v1",
            &disclosure.product_id,
            &disclosure.reference,
        )
            .encode(),
    )
}

/// What one peer should be sent now: the disclosure, or a withdrawal of the
/// one it holds. `None` when it already holds what it should.
fn wanted(
    disclosure: Option<&Disclosure>,
    current: Option<&ProfileWatermark>,
) -> Option<(String, Option<String>, Option<[u8; 32]>)> {
    match (disclosure, current) {
        (Some(disclosure), current) => {
            let digest = disclosure_digest(disclosure);
            if current.is_some_and(|watermark| watermark.digest == Some(digest)) {
                return None;
            }
            Some((
                disclosure.product_id.clone(),
                Some(disclosure.reference.clone()),
                Some(digest),
            ))
        }
        (None, Some(watermark)) if watermark.digest.is_some() => {
            Some((watermark.discloser_product_id.clone(), None, None))
        }
        (None, _) => None,
    }
}

/// A queued reference whose statement lifetime is over.
fn lapsed(entry: &Outgoing, now: u64) -> bool {
    matches!(entry.kind, OutgoingKind::ProfileReference(_))
        && entry
            .statement
            .expiry
            .is_none_or(|expiry| (expiry >> 32) <= now)
}

impl NativeChatActor {
    /// Queue a profile reference (or withdrawal) for every ready peer whose
    /// watermark differs from the user's current disclosure, as far as the
    /// outbox has room. `true` when anything was queued.
    pub(in crate::runtime::native_chat) async fn publish_profile_reference(
        self: &Arc<Self>,
        context: &NativeChatContext,
    ) -> Result<bool, Error> {
        context.require_current()?;
        if self
            .store
            .read(|state| state.boundary.legacy_pending)
            .await?
        {
            return Ok(false);
        }
        self.retire_lapsed_profile_references(context).await?;
        let disclosure = read_disclosure(&*context.services.platform, profile_owner(context))
            .await
            .map_err(|_| Error::StorageUnavailable)?;
        let stale = self
            .store
            .read({
                let disclosure = disclosure.clone();
                move |state| {
                    state
                        .peers
                        .iter()
                        .filter(|peer| peer.ready())
                        .filter(|peer| {
                            let current = state
                                .profile_shared
                                .iter()
                                .find(|watermark| watermark.peer == peer.identity);
                            wanted(disclosure.as_ref(), current).is_some()
                        })
                        .map(|peer| peer.identity)
                        .collect::<Vec<_>>()
                }
            })
            .await?;
        if stale.is_empty() {
            return Ok(false);
        }
        require_authorized(context, &self.product).await?;
        let actor = self.clone();
        let valid = context.session_valid.clone();
        self.store
            .update(move |state| {
                if !valid() {
                    return Err(Error::NotConnected);
                }
                let now = current_unix_secs().saturating_mul(1000);
                let mut queued = false;
                for identity in stale {
                    let peer = state.peer(&identity)?.clone();
                    if !peer.ready() {
                        continue;
                    }
                    let current = state
                        .profile_shared
                        .iter()
                        .find(|watermark| watermark.peer == identity);
                    let Some((discloser, reference, digest)) = wanted(disclosure.as_ref(), current)
                    else {
                        continue;
                    };
                    // Later than anything sent to this peer before, even
                    // after the clock steps back, so its host can order them.
                    let timestamp = current.map_or(now, |watermark| {
                        now.max(watermark.timestamp.saturating_add(1))
                    });
                    let tag = hash(&(identity, &discloser, &reference, timestamp).encode());
                    let request_id = format!("profile-{}", hex::encode(&tag[..8]));
                    let bytes = wire::encode_profile_reference_message(
                        &request_id,
                        timestamp,
                        &discloser,
                        reference.as_deref(),
                    )
                    .map_err(|_| Error::InvalidRequest)?;
                    let messages = Zeroizing::new(vec![bytes]);
                    let statement = actor.multi_statement(
                        state,
                        &peer,
                        &peer.active_devices(),
                        &request_id,
                        &messages,
                    )?;
                    // Only the newest disclosure is worth delivering.
                    state.outbox.retain(|entry| {
                        entry.peer != identity
                            || !matches!(entry.kind, OutgoingKind::ProfileReference(_))
                    });
                    match state.queue(Outgoing {
                        peer: identity,
                        request_id,
                        digest: hash(&messages.encode()),
                        kind: OutgoingKind::ProfileReference(tag),
                        roster_revision: peer.revision,
                        statement,
                        last_attempt: 0,
                    }) {
                        Ok(()) => queued = true,
                        // No room: this peer and the rest keep their
                        // watermarks, so a later reconcile retries them.
                        Err(Error::StorageUnavailable) => break,
                        Err(error) => return Err(error),
                    }
                    state
                        .profile_shared
                        .retain(|watermark| watermark.peer != identity);
                    state.profile_shared.push(ProfileWatermark {
                        peer: identity,
                        digest,
                        discloser_product_id: discloser,
                        timestamp,
                    });
                }
                Ok(queued)
            })
            .await
    }

    /// Drop queued references whose statement lifetime is over. Their
    /// watermarks stay, so the same disclosure is not queued again: a peer
    /// that did not acknowledge it in a lifetime is not helped by another
    /// signature, only a changed disclosure is sent again.
    pub(super) async fn retire_lapsed_profile_references(
        &self,
        context: &NativeChatContext,
    ) -> Result<(), Error> {
        let now = current_unix_secs();
        if !self
            .store
            .read(move |state| state.outbox.iter().any(|entry| lapsed(entry, now)))
            .await?
        {
            return Ok(());
        }
        let valid = context.session_valid.clone();
        self.store
            .update(move |state| {
                if !valid() {
                    return Err(Error::NotConnected);
                }
                state.outbox.retain(|entry| !lapsed(entry, now));
                Ok(())
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disclosure(reference: &str) -> Disclosure {
        Disclosure {
            product_id: "seity.dot".into(),
            reference: reference.into(),
        }
    }

    #[test]
    fn a_peer_is_sent_only_what_it_does_not_hold() {
        let current = disclosure("seity-contacts:v1:aa");
        let held = ProfileWatermark {
            peer: [1; 32],
            digest: Some(disclosure_digest(&current)),
            discloser_product_id: "seity.dot".into(),
            timestamp: 1,
        };
        assert!(wanted(Some(&current), Some(&held)).is_none());
        let (_, reference, _) = wanted(Some(&disclosure("seity-contacts:v1:bb")), Some(&held))
            .expect("a replacement is sent");
        assert_eq!(reference.as_deref(), Some("seity-contacts:v1:bb"));
        let (discloser, reference, digest) =
            wanted(None, Some(&held)).expect("a withdrawal is sent to a holder");
        assert_eq!(
            (discloser.as_str(), reference, digest),
            ("seity.dot", None, None)
        );
        let withdrawn = ProfileWatermark {
            digest: None,
            ..held
        };
        assert!(
            wanted(None, Some(&withdrawn)).is_none(),
            "a withdrawal is sent once"
        );
        assert!(
            wanted(Some(&current), Some(&withdrawn)).is_some(),
            "a withdrawn peer is sent a new disclosure"
        );
        assert!(
            wanted(None, None).is_none(),
            "nothing to withdraw from a new peer"
        );
        assert!(
            wanted(Some(&current), None).is_some(),
            "a new peer is sent the disclosure"
        );
    }
}
