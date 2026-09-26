// SPDX-License-Identifier: AGPL-3.0-only
//! Host-originated profile references: the user's disclosed reference, sealed
//! to each established peer's devices and handed to the product as opaque
//! prepared statements, like payments and rich files.
//!
//! A per-peer watermark records what this Host last queued for that peer, so
//! the initial share, a new contact, a replacement and a withdrawal are one
//! reconcile: every peer whose watermark differs from the disclosure is sent
//! the disclosure. The watermark advances when the message is queued.

use super::*;
use crate::runtime::native_chat::background::require_authorized;
use crate::runtime::profile::{Disclosure, read_disclosure};

/// What this Host last queued to one peer.
#[derive(Clone, PartialEq, Eq, Encode, Decode)]
pub(super) struct ProfileWatermark {
    pub(super) peer: [u8; 32],
    /// Digest of the disclosure sent, identifying it without keeping it.
    pub(super) digest: [u8; 32],
    /// Product that disclosed it, repeated on a withdrawal.
    pub(super) discloser_product_id: String,
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
            if current.is_some_and(|watermark| watermark.digest == digest) {
                return None;
            }
            Some((
                disclosure.product_id.clone(),
                Some(disclosure.reference.clone()),
                Some(digest),
            ))
        }
        (None, Some(watermark)) => Some((watermark.discloser_product_id.clone(), None, None)),
        (None, None) => None,
    }
}

impl NativeChatActor {
    /// Queue a profile reference (or withdrawal) for every ready peer whose
    /// watermark differs from the user's current disclosure.
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
        let disclosure = read_disclosure(&*context.services.platform)
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
                    let tag = hash(&(identity, &discloser, &reference).encode());
                    let request_id = format!("profile-{}", hex::encode(&tag[..8]));
                    let bytes = wire::encode_profile_reference_message(
                        &request_id,
                        current_unix_secs().saturating_mul(1000),
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
                    state.queue(Outgoing {
                        peer: identity,
                        request_id,
                        digest: hash(&messages.encode()),
                        kind: OutgoingKind::ProfileReference(tag),
                        roster_revision: peer.revision,
                        statement,
                        last_attempt: 0,
                    })?;
                    state
                        .profile_shared
                        .retain(|watermark| watermark.peer != identity);
                    if let Some(digest) = digest {
                        state.profile_shared.push(ProfileWatermark {
                            peer: identity,
                            digest,
                            discloser_product_id: discloser,
                        });
                    }
                }
                Ok(())
            })
            .await?;
        Ok(true)
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
            digest: disclosure_digest(&current),
            discloser_product_id: "seity.dot".into(),
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
