// SPDX-License-Identifier: AGPL-3.0-only
//! Host-originated profile references: the user's disclosed reference, sealed
//! to each established peer's devices and handed to the product as opaque
//! prepared statements, like payments and rich files.
//!
//! A per-peer watermark records what this Host last queued for that peer, so
//! the initial share, a new contact, a replacement and a withdrawal are one
//! publish: every ready peer whose watermark differs from the disclosure is
//! sent the disclosure. The watermark advances when the message is queued.
//! Each frame to a peer is timestamped later than the one before it, so the
//! peer's host keeps the newest whatever order it opens them in.
//!
//! A publish runs when the chat product initializes or reconciles, after any
//! Chat request in which a peer became ready, and when the disclosure changes
//! while the chat is open (`NativeChatRegistry::relay_profile_disclosure`).
//! Publishes on one actor run one at a time, so one that read an older
//! disclosure never queues it after a newer one.
//!
//! Delivery is best effort. References have their own outbox budget, one per
//! peer, so they never take a slot user traffic needs; a reference that finds
//! no room is left for a later publish. A queued reference is offered for one
//! statement lifetime. If it lapses unacknowledged, it is signed again and
//! offered to a ready peer for another lifetime, up to
//! [`MAX_PROFILE_ATTEMPTS`] frames per peer and disclosure: a host that does
//! not know the content type never acknowledges it, so it costs at most that
//! many statements each time the disclosure changes.

use super::*;
use crate::runtime::native_chat::background::require_authorized;
use crate::runtime::profile::{Disclosure, ProfileOwner, read_disclosure};

/// Frames signed for one disclosure to one peer, the first included, before
/// the Host stops offering it until the disclosure changes.
pub(super) const MAX_PROFILE_ATTEMPTS: u8 = 3;

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
    /// Frames signed for this digest, the one sent included.
    pub(super) attempts: u8,
    /// The frame sent lapsed without an acknowledgement.
    pub(super) lapsed: bool,
}

/// A watermark as written from 571f348f4 until lapsed frames were resent: no
/// attempt count and no lapse marker.
#[derive(Decode)]
struct SingleAttemptWatermark {
    peer: [u8; 32],
    digest: Option<[u8; 32]>,
    discloser_product_id: String,
    timestamp: u64,
}

/// A watermark as written before frames were ordered: peer, disclosure
/// digest, discloser. No timestamp, and no way to record a withdrawal.
type LegacyWatermark = ([u8; 32], [u8; 32], String);

/// Decode the trailing watermark list of a Chat state snapshot, in the
/// current layout or either earlier one.
///
/// A single-attempt watermark counts as one attempt that did not lapse. The
/// layout cannot tell a frame that was acknowledged from one that lapsed and
/// was dropped, so, as when it was written, the peer is sent nothing more
/// until the disclosure changes.
///
/// Legacy watermarks are dropped rather than carried over. They were written
/// when contacts' hosts kept received references in a slot that is no longer
/// read, so no contact holds what they record, and the next publish has to
/// send every contact the disclosure again. Each layout must consume the whole
/// list; anything else is corruption.
pub(super) fn decode_watermarks(
    bytes: &[u8],
) -> Result<Vec<ProfileWatermark>, parity_scale_codec::Error> {
    use parity_scale_codec::DecodeAll;
    if let Ok(current) = Vec::<ProfileWatermark>::decode_all(&mut &bytes[..]) {
        return Ok(current);
    }
    if let Ok(single) = Vec::<SingleAttemptWatermark>::decode_all(&mut &bytes[..]) {
        return Ok(single
            .into_iter()
            .map(|watermark| ProfileWatermark {
                peer: watermark.peer,
                digest: watermark.digest,
                discloser_product_id: watermark.discloser_product_id,
                timestamp: watermark.timestamp,
                attempts: 1,
                lapsed: false,
            })
            .collect());
    }
    Vec::<LegacyWatermark>::decode_all(&mut &bytes[..])?;
    Ok(Vec::new())
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

/// One frame to queue for a peer.
#[derive(Debug, PartialEq, Eq)]
struct Frame {
    discloser: String,
    /// `None` withdraws.
    reference: Option<String>,
    digest: Option<[u8; 32]>,
    /// Frames signed for this digest, this one included.
    attempts: u8,
}

/// What one peer should be sent now, given the user's disclosure and its
/// digest: the disclosure, a withdrawal of the one it holds, or the frame it
/// was last sent again, once that lapsed with attempts to spare. `None` when
/// it holds what it should, is still offered it, or has had every attempt.
fn wanted(
    disclosure: Option<&(Disclosure, [u8; 32])>,
    current: Option<&ProfileWatermark>,
) -> Option<Frame> {
    let (discloser, reference, digest) = match (disclosure, current) {
        (Some((disclosure, digest)), _) => (
            &disclosure.product_id,
            Some(&disclosure.reference),
            Some(*digest),
        ),
        (None, Some(watermark)) => (&watermark.discloser_product_id, None, None),
        (None, None) => return None,
    };
    let attempts = match current {
        Some(watermark) if watermark.digest == digest => {
            if !watermark.lapsed || watermark.attempts >= MAX_PROFILE_ATTEMPTS {
                return None;
            }
            watermark.attempts + 1
        }
        _ => 1,
    };
    Some(Frame {
        discloser: discloser.clone(),
        reference: reference.cloned(),
        digest,
        attempts,
    })
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
    /// watermark differs from the user's current disclosure, or whose last
    /// frame lapsed with attempts to spare, as far as the outbox has room.
    /// `true` when anything was queued.
    pub(in crate::runtime::native_chat) async fn publish_profile_reference(
        self: &Arc<Self>,
        context: &NativeChatContext,
    ) -> Result<bool, Error> {
        context.require_current()?;
        // Each publish reads the disclosure and then queues it; two at once
        // could queue the older one last.
        let _publishing = self.profile_gate.lock().await;
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
            .map_err(|_| Error::StorageUnavailable)?
            .map(|disclosure| {
                let digest = disclosure_digest(&disclosure);
                (disclosure, digest)
            });
        let stale = self
            .store
            .read(|state| {
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
                    let Some(frame) = wanted(disclosure.as_ref(), current) else {
                        continue;
                    };
                    // Later than anything sent to this peer before, even
                    // after the clock steps back, so its host can order them.
                    // A resend is a new frame too, with its own request id.
                    let timestamp = current.map_or(now, |watermark| {
                        now.max(watermark.timestamp.saturating_add(1))
                    });
                    let tag =
                        hash(&(identity, &frame.discloser, &frame.reference, timestamp).encode());
                    let request_id = format!("profile-{}", hex::encode(&tag[..8]));
                    let bytes = wire::encode_profile_reference_message(
                        &request_id,
                        timestamp,
                        &frame.discloser,
                        frame.reference.as_deref(),
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
                        // watermarks, so a later publish retries them.
                        Err(Error::StorageUnavailable) => break,
                        Err(error) => return Err(error),
                    }
                    state
                        .profile_shared
                        .retain(|watermark| watermark.peer != identity);
                    state.profile_shared.push(ProfileWatermark {
                        peer: identity,
                        digest: frame.digest,
                        discloser_product_id: frame.discloser,
                        timestamp,
                        attempts: frame.attempts,
                        lapsed: false,
                    });
                }
                Ok(queued)
            })
            .await
    }

    /// Publish for a trigger that has no caller to answer: a peer became
    /// ready, the disclosure changed, or a reconcile. A failure waits for the
    /// next publish.
    pub(in crate::runtime::native_chat) async fn relay_profile_reference(
        self: &Arc<Self>,
        context: &NativeChatContext,
    ) {
        if let Err(error) = self.publish_profile_reference(context).await {
            tracing::debug!(?error, "native Chat profile relay deferred");
        }
    }

    /// Peers the relay does not reach yet, which a Chat request may make
    /// ready. A peer is never ready in the request that adds it.
    pub(in crate::runtime::native_chat) async fn unready_peers(
        &self,
    ) -> Result<Vec<[u8; 32]>, Error> {
        self.store
            .read(|state| {
                state
                    .peers
                    .iter()
                    .filter(|peer| !peer.ready())
                    .map(|peer| peer.identity)
                    .collect()
            })
            .await
    }

    /// Relay to the peers of `unready` that are ready now.
    pub(in crate::runtime::native_chat) async fn relay_to_newly_ready(
        self: &Arc<Self>,
        context: &NativeChatContext,
        unready: &[[u8; 32]],
    ) {
        if unready.is_empty() {
            return;
        }
        let became_ready = self
            .store
            .read(|state| {
                state
                    .peers
                    .iter()
                    .any(|peer| peer.ready() && unready.contains(&peer.identity))
            })
            .await;
        if became_ready.unwrap_or(false) {
            self.relay_profile_reference(context).await;
        }
    }

    /// Drop queued references whose statement lifetime is over and mark their
    /// watermarks lapsed, so the publish may sign the frame again.
    async fn retire_lapsed_profile_references(
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
                // A peer has one reference queued at most, the one its
                // watermark records.
                for watermark in &mut state.profile_shared {
                    watermark.lapsed |= state
                        .outbox
                        .iter()
                        .any(|entry| entry.peer == watermark.peer && lapsed(entry, now));
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

    fn disclosure(reference: &str) -> (Disclosure, [u8; 32]) {
        let disclosure = Disclosure {
            product_id: "seity.dot".into(),
            reference: reference.into(),
        };
        let digest = disclosure_digest(&disclosure);
        (disclosure, digest)
    }

    #[test]
    fn a_peer_is_sent_only_what_it_does_not_hold() {
        let current = disclosure("seity-contacts:v1:aa");
        let held = ProfileWatermark {
            peer: [1; 32],
            digest: Some(current.1),
            discloser_product_id: "seity.dot".into(),
            timestamp: 1,
            attempts: 1,
            lapsed: false,
        };
        assert!(wanted(Some(&current), Some(&held)).is_none());
        let replacement = wanted(Some(&disclosure("seity-contacts:v1:bb")), Some(&held))
            .expect("a replacement is sent");
        assert_eq!(
            (replacement.reference.as_deref(), replacement.attempts),
            (Some("seity-contacts:v1:bb"), 1)
        );
        assert_eq!(
            wanted(None, Some(&held)).expect("a withdrawal is sent to a holder"),
            Frame {
                discloser: "seity.dot".into(),
                reference: None,
                digest: None,
                attempts: 1,
            }
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

    #[test]
    fn a_lapsed_frame_is_sent_again_until_its_attempts_run_out() {
        let current = disclosure("seity-contacts:v1:aa");
        let lapsed_watermark = |digest, attempts| ProfileWatermark {
            peer: [1; 32],
            digest,
            discloser_product_id: "seity.dot".into(),
            timestamp: 1,
            attempts,
            lapsed: true,
        };
        let resent = wanted(Some(&current), Some(&lapsed_watermark(Some(current.1), 1)))
            .expect("a lapsed disclosure is sent again");
        assert_eq!(
            (resent.digest, resent.attempts),
            (Some(current.1), 2),
            "as another attempt at the same disclosure"
        );
        assert!(
            wanted(
                Some(&current),
                Some(&lapsed_watermark(Some(current.1), MAX_PROFILE_ATTEMPTS))
            )
            .is_none(),
            "not once its attempts are spent"
        );
        assert_eq!(
            wanted(
                Some(&disclosure("seity-contacts:v1:bb")),
                Some(&lapsed_watermark(Some(current.1), MAX_PROFILE_ATTEMPTS))
            )
            .expect("a new disclosure is sent")
            .attempts,
            1,
            "with attempts of its own"
        );
        assert_eq!(
            wanted(None, Some(&lapsed_watermark(None, 1)))
                .expect("a lapsed withdrawal is sent again")
                .attempts,
            2
        );
        assert!(wanted(None, Some(&lapsed_watermark(None, MAX_PROFILE_ATTEMPTS))).is_none());
    }
}
