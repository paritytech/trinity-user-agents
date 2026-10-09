// SPDX-License-Identifier: AGPL-3.0-only
//! Host-originated profile references: the user's disclosed reference, sealed
//! to each established peer's devices and handed to the product as opaque
//! prepared statements, like payments and rich files.
//!
//! A per-peer watermark records what this Host last queued for that peer, so
//! the initial share, a new contact, a replacement and a withdrawal are one
//! publish: every ready peer whose watermark differs from the disclosure is
//! sent the disclosure. Each `profile.disclose` call is a new revision, so
//! disclosing the same reference again (its record changed) is sent to every
//! ready peer anew; automatic publishes never resend a revision already sent.
//! The watermark advances when the message is queued.
//! Each frame to a peer is timestamped later than the one before it, so the
//! peer's host keeps the newest whatever order it opens them in.
//!
//! A publish runs when the chat product initializes or reconciles, after any
//! Chat request in which a peer became ready, and when the disclosure changes
//! while the chat is open (`NativeChatRegistry::relay_profile_disclosure`).
//! Publishes on one actor run one at a time, so one that read an older
//! disclosure never queues it after a newer one.
//!
//! Delivery is best effort. References have their own fixed outbox budget,
//! so they never take a slot user traffic needs; a reference that finds
//! no room is left for a later publish. A queued reference is offered for one
//! statement lifetime. If it lapses unacknowledged, it is signed again and
//! offered to a ready peer for another lifetime, up to
//! [`MAX_PROFILE_ATTEMPTS`] frames per peer and disclosure: a host that does
//! not know the content type never acknowledges it, so it costs at most that
//! many statements each time the user discloses or retracts.

use super::*;
use crate::runtime::native_chat::background::require_authorized;
use crate::runtime::profile::{Disclosure, ProfileOwner, ProfileScope, read_disclosure_state};

/// Frames signed for one disclosure to one peer, the first included, before
/// the Host stops offering it until the disclosure changes.
pub(super) const MAX_PROFILE_ATTEMPTS: u8 = 3;

pub(super) const WATERMARK_MARKER: [u8; 4] = [0xff, b'P', b'R', 3];

/// What this Host last queued to one peer.
#[derive(Clone, PartialEq, Eq, Encode, Decode)]
pub(super) struct ProfileWatermark {
    pub(super) peer: [u8; 32],
    pub(super) scope: ProfileScope,
    pub(super) revision: u64,
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
    /// Device roster the last frame was sealed to, independent of disclosure revision.
    pub(super) roster_revision: u64,
}

/// App-scoped watermarks written before independent personal grants.
#[derive(Decode)]
struct AppWatermark {
    peer: [u8; 32],
    digest: Option<[u8; 32]>,
    discloser_product_id: String,
    timestamp: u64,
    attempts: u8,
    lapsed: bool,
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
    peers: &[Peer],
    outbox: &[Outgoing],
) -> Result<Vec<ProfileWatermark>, parity_scale_codec::Error> {
    use parity_scale_codec::DecodeAll;
    if let Some(current) = bytes.strip_prefix(&WATERMARK_MARKER) {
        let watermarks = Vec::<ProfileWatermark>::decode_all(&mut &current[..])?;
        if watermarks.len() > MAX_PEERS * 2 {
            return Err("too many profile watermarks".into());
        }
        return Ok(watermarks);
    }
    let roster_revision = |identity: [u8; 32], scope: ProfileScope| {
        outbox.iter()
            .find(|entry| entry.peer == identity && entry.kind.profile_scope() == Some(scope))
            .map(|entry| entry.roster_revision)
            .or_else(|| peers.iter().find(|peer| peer.identity == identity).map(|peer| peer.revision))
            .unwrap_or(0)
    };
    if let Some(previous) = bytes.strip_prefix(&[0xff, b'P', b'R', 2]) {
        type PreviousWatermark = ([u8; 32], ProfileScope, u64, Option<[u8; 32]>, String, u64, u8, bool);
        let previous = Vec::<PreviousWatermark>::decode_all(&mut &previous[..])?;
        if previous.len() > MAX_PEERS * 2 {
            return Err("too many profile watermarks".into());
        }
        return Ok(previous.into_iter().map(
            |(peer, scope, revision, digest, discloser_product_id, timestamp, attempts, lapsed)| {
                ProfileWatermark {
                    peer, scope, revision, digest, discloser_product_id, timestamp, attempts, lapsed,
                    roster_revision: roster_revision(peer, scope),
                }
            },
        ).collect());
    }
    if let Ok(app) = Vec::<AppWatermark>::decode_all(&mut &bytes[..]) {
        return Ok(app
            .into_iter()
            .map(|watermark| ProfileWatermark {
                peer: watermark.peer,
                scope: ProfileScope::App,
                revision: 0,
                digest: watermark.digest,
                discloser_product_id: watermark.discloser_product_id,
                timestamp: watermark.timestamp,
                attempts: watermark.attempts,
                lapsed: watermark.lapsed,
                roster_revision: roster_revision(watermark.peer, ProfileScope::App),
            })
            .collect());
    }
    if let Ok(single) = Vec::<SingleAttemptWatermark>::decode_all(&mut &bytes[..]) {
        return Ok(single
            .into_iter()
            .map(|watermark| ProfileWatermark {
                peer: watermark.peer,
                scope: ProfileScope::App,
                revision: 0,
                digest: watermark.digest,
                discloser_product_id: watermark.discloser_product_id,
                timestamp: watermark.timestamp,
                attempts: 1,
                lapsed: false,
                roster_revision: roster_revision(watermark.peer, ProfileScope::App),
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

/// What a watermark records a disclosure by. Each `profile.disclose` call has
/// its own revision and so its own digest, and starts a new round even for
/// the same reference; automatic publishes of one disclosure share it. A
/// disclosure stored before revisions keeps the digest it was sent under.
pub(super) fn disclosure_digest(disclosure: &Disclosure) -> [u8; 32] {
    if disclosure.revision == 0 {
        return hash(
            &(
                b"native-chat-profile-v1",
                &disclosure.product_id,
                &disclosure.reference,
            )
                .encode(),
        );
    }
    hash(
        &(
            b"native-chat-profile-v2",
            &disclosure.product_id,
            &disclosure.reference,
            disclosure.revision,
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

fn next_attempt(
    disclosure: Option<&(Disclosure, [u8; 32])>,
    current: Option<&ProfileWatermark>,
    revision: u64,
    roster_revision: u64,
) -> Option<u8> {
    if disclosure.is_none() && current.is_none() {
        return None;
    }
    let digest = disclosure.map(|(_, digest)| *digest);
    match current {
        Some(watermark)
            if watermark.digest == digest
                && (watermark.scope == ProfileScope::App || watermark.revision == revision)
                && watermark.roster_revision == roster_revision =>
        {
            (watermark.lapsed && watermark.attempts < MAX_PROFILE_ATTEMPTS)
                .then(|| watermark.attempts + 1)
        }
        _ => Some(1),
    }
}

/// What one peer should be sent now, given the user's disclosure and its
/// digest: the disclosure, a withdrawal of the one it holds, or the frame it
/// was last sent again, once that lapsed with attempts to spare. `None` when
/// it holds what it should, is still offered it, or has had every attempt.
fn wanted(
    disclosure: Option<&(Disclosure, [u8; 32])>,
    current: Option<&ProfileWatermark>,
    revision: u64,
    roster_revision: u64,
) -> Option<Frame> {
    let attempts = next_attempt(disclosure, current, revision, roster_revision)?;
    let (discloser, reference, digest) = match (disclosure, current) {
        (Some((disclosure, digest)), _) => (
            &disclosure.product_id,
            Some(&disclosure.reference),
            Some(*digest),
        ),
        (None, Some(watermark)) => (&watermark.discloser_product_id, None, None),
        (None, None) => return None,
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
    entry.kind.profile_scope().is_some()
        && entry
            .statement
            .expiry
            .is_none_or(|expiry| (expiry >> 32) <= now)
}

pub(super) fn superseded(
    entry: &Outgoing,
    watermarks: &[ProfileWatermark],
    peers: &[Peer],
    disclosure: Option<&(Disclosure, [u8; 32])>,
    product: &str,
    revision: u64,
) -> bool {
    let Some(scope) = entry.kind.profile_scope() else {
        return false;
    };
    if peers.iter().find(|peer| peer.identity == entry.peer)
        .is_none_or(|peer| peer.revision != entry.roster_revision)
    {
        return true;
    }
    let Some(current) = watermarks
        .iter()
        .find(|watermark| watermark.peer == entry.peer && watermark.scope == scope)
    else {
        return scope == ProfileScope::Personal;
    };
    let desired = disclosure
        .filter(|(disclosure, _)| disclosure.grants(scope, product, &entry.peer))
        .map(|(_, digest)| *digest);
    current.digest != desired || (scope == ProfileScope::Personal && current.revision != revision)
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
        let _profile_state = context.services.profile_state_gate.lock().await;
        if self
            .store
            .read(|state| state.boundary.legacy_pending)
            .await?
        {
            return Ok(false);
        }
        self.retire_lapsed_profile_references(context).await?;
        let (revision, disclosure) =
            read_disclosure_state(&*context.services.platform, profile_owner(context))
                .await
                .map_err(|_| Error::StorageUnavailable)?;
        let disclosure = disclosure.map(|disclosure| {
            let digest = disclosure_digest(&disclosure);
            (disclosure, digest)
        });
        // Remove superseded shares even for an unready peer. Keep its
        // watermark so the withdrawal is still due after restart.
        let obsolete = self
            .store
            .read(|state| {
                state.outbox.iter().any(|entry| {
                    superseded(
                        entry,
                        &state.profile_shared,
                        &state.peers,
                        disclosure.as_ref(),
                        &self.product,
                        revision,
                    )
                })
            })
            .await?;
        if obsolete {
            let current_disclosure = disclosure.clone();
            let product = self.product.clone();
            let valid = context.session_valid.clone();
            self.store
                .update(move |state| {
                    if !valid() {
                        return Err(Error::NotConnected);
                    }
                    state.outbox.retain(|entry| {
                        !superseded(
                            entry,
                            &state.profile_shared,
                            &state.peers,
                            current_disclosure.as_ref(),
                            &product,
                            revision,
                        )
                    });
                    Ok(())
                })
                .await?;
        }
        let stale = self
            .store
            .read(|state| {
                let mut stale = Vec::new();
                for peer in state.peers.iter().filter(|peer| peer.ready()) {
                    for scope in [ProfileScope::App, ProfileScope::Personal] {
                        let current = state.profile_shared.iter().find(|watermark| {
                            watermark.peer == peer.identity && watermark.scope == scope
                        });
                        let granted = disclosure.as_ref().filter(|(disclosure, _)| {
                            disclosure.grants(scope, &self.product, &peer.identity)
                        });
                        if next_attempt(granted, current, revision, peer.revision).is_some() {
                            stale.push((peer.identity, scope));
                        }
                    }
                }
                stale
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
                for (identity, scope) in stale {
                    let peer = state.peer(&identity)?.clone();
                    if !peer.ready() {
                        continue;
                    }
                    let current = state
                        .profile_shared
                        .iter()
                        .find(|watermark| watermark.peer == identity && watermark.scope == scope);
                    let granted = disclosure.as_ref().filter(|(disclosure, _)| {
                        disclosure.grants(scope, &actor.product, &identity)
                    });
                    let Some(frame) = wanted(granted, current, revision, peer.revision) else {
                        continue;
                    };
                    // Later than anything sent to this peer before, even
                    // after the clock steps back, so its host can order them.
                    // A resend is a new frame too, with its own request id.
                    let timestamp = current.map_or(now, |watermark| {
                        now.max(watermark.timestamp.saturating_add(1))
                    });
                    // Public request ids must not reveal a guessable reference digest
                    // or correlate the same disclosure across product actors.
                    let tag = hash(&Zeroizing::new((
                        b"native-chat-profile-tag-v1", &state.secret.0, identity, scope,
                        revision, &frame.discloser, &frame.reference, timestamp,
                    ).encode()));
                    let request_id = format!("profile-{}", hex::encode(&tag[..8]));
                    let bytes = match scope {
                        ProfileScope::App => wire::encode_profile_reference_message(
                            &request_id,
                            timestamp,
                            &frame.discloser,
                            frame.reference.as_deref(),
                        ),
                        ProfileScope::Personal => wire::encode_personal_profile_reference_message(
                            &request_id,
                            timestamp,
                            revision,
                            &frame.discloser,
                            frame.reference.as_deref(),
                        ),
                    }
                    .map_err(|_| Error::InvalidRequest)?;
                    let messages = Zeroizing::new(vec![bytes]);
                    let statement = actor.multi_statement(
                        state,
                        &peer,
                        &peer.active_devices(),
                        &request_id,
                        &messages,
                    )?;
                    // Each scope keeps its own pending share or withdrawal.
                    state.outbox.retain(|entry| {
                        entry.peer != identity || entry.kind.profile_scope() != Some(scope)
                    });
                    match state.queue(Outgoing {
                        peer: identity,
                        request_id,
                        digest: hash(&messages.encode()),
                        kind: match scope {
                            ProfileScope::App => OutgoingKind::ProfileReference(tag),
                            ProfileScope::Personal => OutgoingKind::PersonalProfileReference(tag),
                        },
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
                        .retain(|watermark| watermark.peer != identity || watermark.scope != scope);
                    state.profile_shared.push(ProfileWatermark {
                        peer: identity,
                        scope,
                        revision: if scope == ProfileScope::Personal {
                            revision
                        } else {
                            0
                        },
                        digest: frame.digest,
                        discloser_product_id: frame.discloser,
                        timestamp,
                        attempts: frame.attempts,
                        lapsed: false,
                        roster_revision: peer.revision,
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

    /// The name this Chat's roster holds for `peer`: resolved and verified
    /// by the host when the contact was bound or first authenticated. `None`
    /// when `peer` is not a contact, has no name, or the store is unreadable.
    pub(in crate::runtime::native_chat) async fn contact_username(
        &self,
        peer: &[u8; 32],
    ) -> Option<String> {
        self.store
            .read(|state| state.peer(peer).ok().and_then(|peer| peer.username.clone()))
            .await
            .ok()
            .flatten()
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
                // A peer has at most one pending frame per scope.
                for watermark in &mut state.profile_shared {
                    watermark.lapsed |= state.outbox.iter().any(|entry| {
                        entry.peer == watermark.peer
                            && entry.kind.profile_scope() == Some(watermark.scope)
                            && lapsed(entry, now)
                    });
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
            revision: 1,
            all_chat_apps: true,
            app_products: Vec::new(),
            contacts: Vec::new(),
        };
        let digest = disclosure_digest(&disclosure);
        (disclosure, digest)
    }

    #[test]
    fn a_peer_is_sent_only_what_it_does_not_hold() {
        let current = disclosure("seity-contacts:v1:aa");
        let held = ProfileWatermark {
            peer: [1; 32],
            scope: ProfileScope::App,
            revision: 0,
            digest: Some(current.1),
            discloser_product_id: "seity.dot".into(),
            timestamp: 1,
            attempts: 1,
            lapsed: false,
            roster_revision: 0,
        };
        assert!(wanted(Some(&current), Some(&held), 0, 0).is_none());
        let replacement = wanted(Some(&disclosure("seity-contacts:v1:bb")), Some(&held), 0, 0)
            .expect("a replacement is sent");
        assert_eq!(
            (replacement.reference.as_deref(), replacement.attempts),
            (Some("seity-contacts:v1:bb"), 1)
        );
        assert_eq!(
            wanted(None, Some(&held), 0, 0).expect("a withdrawal is sent to a holder"),
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
            wanted(None, Some(&withdrawn), 0, 0).is_none(),
            "a withdrawal is sent once"
        );
        assert!(
            wanted(Some(&current), Some(&withdrawn), 0, 0).is_some(),
            "a withdrawn peer is sent a new disclosure"
        );
        assert!(
            wanted(None, None, 0, 0).is_none(),
            "nothing to withdraw from a new peer"
        );
        assert!(
            wanted(Some(&current), None, 0, 0).is_some(),
            "a new peer is sent the disclosure"
        );
        assert_eq!(
            wanted(None, Some(&withdrawn), 0, 1).unwrap().attempts, 1,
            "a changed device roster starts a fresh delivery round"
        );
    }

    #[test]
    fn a_lapsed_frame_is_sent_again_until_its_attempts_run_out() {
        let current = disclosure("seity-contacts:v1:aa");
        let lapsed_watermark = |digest, attempts| ProfileWatermark {
            peer: [1; 32],
            scope: ProfileScope::App,
            revision: 0,
            digest,
            discloser_product_id: "seity.dot".into(),
            timestamp: 1,
            attempts,
            lapsed: true,
            roster_revision: 0,
        };
        let resent = wanted(
            Some(&current),
            Some(&lapsed_watermark(Some(current.1), 1)),
            0,
            0,
        )
        .expect("a lapsed disclosure is sent again");
        assert_eq!(
            (resent.digest, resent.attempts),
            (Some(current.1), 2),
            "as another attempt at the same disclosure"
        );
        assert!(
            wanted(
                Some(&current),
                Some(&lapsed_watermark(Some(current.1), MAX_PROFILE_ATTEMPTS)),
                0, 0
            )
            .is_none(),
            "not once its attempts are spent"
        );
        assert_eq!(
            wanted(
                Some(&disclosure("seity-contacts:v1:bb")),
                Some(&lapsed_watermark(Some(current.1), MAX_PROFILE_ATTEMPTS)),
                0, 0
            )
            .expect("a new disclosure is sent")
            .attempts,
            1,
            "with attempts of its own"
        );
        assert_eq!(
            wanted(None, Some(&lapsed_watermark(None, 1)), 0, 0)
                .expect("a lapsed withdrawal is sent again")
                .attempts,
            2
        );
        assert!(wanted(None, Some(&lapsed_watermark(None, MAX_PROFILE_ATTEMPTS)), 0, 0).is_none());
    }

    #[test]
    fn previous_scoped_watermarks_retain_personal_withdrawal_revisions() {
        let mut bytes = vec![0xff, b'P', b'R', 2];
        bytes.extend(vec![(
            [1u8; 32], ProfileScope::Personal, 10u64, None::<[u8; 32]>,
            "seity.dot".to_string(), 91u64, 1u8, false,
        )].encode());
        let migrated = decode_watermarks(&bytes, &[], &[]).unwrap();
        assert_eq!(migrated[0].scope, ProfileScope::Personal);
        assert_eq!(migrated[0].revision, 10);
        assert!(wanted(None, Some(&migrated[0]), 10, 0).is_none());
        assert!(wanted(None, Some(&migrated[0]), 10, 1).is_some());
    }

    #[test]
    fn current_app_watermarks_migrate_without_broadening_or_losing_pending_withdrawal() {
        let current = disclosure("profile:secret");
        let bytes = vec![(
            [1u8; 32],
            Some(current.1),
            "seity.dot".to_string(),
            91u64,
            2u8,
            true,
        )]
        .encode();
        let migrated = decode_watermarks(&bytes, &[], &[]).unwrap();
        assert_eq!(migrated[0].scope, ProfileScope::App);
        assert_eq!(migrated[0].timestamp, 91);
        assert_eq!(
            wanted(Some(&current), Some(&migrated[0]), 9, 0)
                .unwrap()
                .attempts,
            3
        );
        assert_eq!(
            wanted(None, Some(&migrated[0]), 10, 0).unwrap().reference,
            None
        );
        let withdrawn = ProfileWatermark {
            scope: ProfileScope::Personal,
            revision: 10,
            digest: None,
            lapsed: false,
            ..migrated[0].clone()
        };
        assert!(wanted(None, Some(&withdrawn), 10, 0).is_none());
        assert!(
            wanted(None, Some(&withdrawn), 12, 0).is_some(),
            "an actor that missed a regrant must send the newer withdrawal"
        );
    }
}
