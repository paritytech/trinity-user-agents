//! Host channel state: the channel keys this wallet's contacts handed over.
//!
//! One slot per wallet and Chat network, like the profile references it sits
//! beside, holding each peer's pairwise X25519 public key from its bootstrap
//! frame. The pair secret is never stored; it is derived from the wallet's
//! entropy and this key on use (`host_logic::host_channel`).

use parity_scale_codec::{Decode, DecodeAll, Encode};

use crate::platform::{CoreStorage, CoreStorageKey};
use crate::runtime::profile::ProfileOwner;

/// A contact roster is bounded; so is what the host keeps for it.
const MAX_PEERS: usize = 4096;
const VERSION: u8 = 1;

/// One peer's half of a host channel.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub(crate) struct HostChannelPeer {
    /// The peer's authenticated root identity, as the Chat roster names it.
    pub(crate) peer_identity: [u8; 32],
    /// The peer's pairwise X25519 public key for this wallet.
    pub(crate) channel_public_key: [u8; 32],
}

fn key(owner: ProfileOwner) -> CoreStorageKey {
    CoreStorageKey::HostChannelPeers {
        root_public_key: owner.root_public_key,
        genesis_hash: owner.genesis_hash,
    }
}

fn storage_error(error: impl core::fmt::Debug) -> String {
    format!("host channel storage failed: {error:?}")
}

/// Every peer this wallet holds a channel key for.
pub(crate) async fn read_peers(
    storage: &(impl CoreStorage + ?Sized),
    owner: ProfileOwner,
) -> Result<Vec<HostChannelPeer>, String> {
    let Some(raw) = storage
        .read_core_storage(key(owner))
        .await
        .map_err(storage_error)?
    else {
        return Ok(Vec::new());
    };
    let (version, peers) = <(u8, Vec<HostChannelPeer>)>::decode_all(&mut raw.as_slice())
        .map_err(|error| format!("stored host channel peers are unreadable: {error}"))?;
    if version != VERSION || peers.len() > MAX_PEERS {
        return Err("invalid host channel peers".into());
    }
    Ok(peers)
}

/// Record the key `peer_identity` sent, replacing an earlier one. `true`
/// when the stored key changed.
pub(crate) async fn upsert_peer(
    storage: &(impl CoreStorage + ?Sized),
    owner: ProfileOwner,
    peer_identity: [u8; 32],
    channel_public_key: [u8; 32],
) -> Result<bool, String> {
    let mut peers = read_peers(storage, owner).await?;
    match peers
        .iter()
        .position(|peer| peer.peer_identity == peer_identity)
    {
        Some(index) if peers[index].channel_public_key == channel_public_key => return Ok(false),
        Some(index) => peers[index].channel_public_key = channel_public_key,
        None if peers.len() >= MAX_PEERS => return Err("too many host channel peers".into()),
        None => peers.push(HostChannelPeer {
            peer_identity,
            channel_public_key,
        }),
    }
    storage
        .write_core_storage(key(owner), (VERSION, peers).encode())
        .await
        .map_err(storage_error)?;
    Ok(true)
}

// ---- the channel itself: send on a disclosure change, receive in the background ----

use std::collections::HashMap;

use futures::{FutureExt, StreamExt, future::AbortHandle};
use futures_timer::Delay;

use crate::host_logic::host_channel::{
    ChannelKey, KIND_PROFILE_REFERENCE, Pair, ProfileReference,
};
use crate::host_logic::product_account::derive_identity_keypair;
use crate::host_logic::statement_store::{
    decode_signed_statement, parse_new_statements_result, sign_statement_fields,
    statement_fields_from_v01,
};
use crate::runtime::native_chat::NativeChatContext;
use crate::runtime::profile::{read_disclosure_state, record_personal_received_reference};
use crate::runtime::statement_store_rpc;
use crate::unix_time::current_unix_secs;
use truapi::latest::Statement;

/// How long a host message is offered on the store, like a Chat frame.
const LIFETIME_SECS: u64 = 2 * 86_400;
/// How often the receiver re-reads its peers and checks the session.
const REFRESH: core::time::Duration = core::time::Duration::from_secs(60);
/// How long the receiver waits before retrying after the store refused it.
const RETRY: core::time::Duration = core::time::Duration::from_secs(30);
/// Most topics one subscription may carry (the store's filter cap).
const MAX_TOPICS: usize = 128;

fn owner_of(context: &NativeChatContext) -> ProfileOwner {
    ProfileOwner {
        root_public_key: context.session.public_key,
        genesis_hash: context.genesis_hash,
    }
}

/// Every stored peer with the pair this wallet shares with it. A peer whose
/// key does not pair is skipped.
async fn pairs(context: &NativeChatContext) -> Result<Vec<([u8; 32], Pair)>, String> {
    let owner = owner_of(context);
    Ok(read_peers(&*context.services.platform, owner)
        .await?
        .into_iter()
        .filter_map(|peer| {
            ChannelKey::derive(&context.entropy, &context.genesis_hash, &peer.peer_identity)
                .pair(&peer.channel_public_key)
                .ok()
                .map(|pair| (peer.peer_identity, pair))
        })
        .collect())
}

/// The signed statements that carry the current disclosure to every
/// bootstrapped peer, or to `only`: the reference where the disclosure
/// grants it, a withdrawal where it does not. The disclosure's own sequence
/// is the revision, so a receiver keeps the newest whatever order it opens
/// them in. Pure apart from reading storage; `publish` submits them.
pub(crate) async fn build(
    context: &NativeChatContext,
    only: Option<[u8; 32]>,
) -> Result<Vec<([u8; 32], Vec<u8>)>, String> {
    context
        .require_current()
        .map_err(|_| "session changed".to_string())?;
    let owner = owner_of(context);
    let (revision, disclosure) = read_disclosure_state(&*context.services.platform, owner).await?;
    if revision == 0 {
        // Nothing was ever disclosed; there is nothing to send or withdraw.
        return Ok(Vec::new());
    }
    let signer = derive_identity_keypair(&context.entropy, &context.network_suffix)
        .map_err(|error| format!("host channel signer: {error}"))?;
    let secret = zeroize::Zeroizing::new(signer.secret.to_bytes());
    let public = signer.public.to_bytes();
    let mut statements = Vec::new();
    for (peer, pair) in pairs(context).await? {
        if only.is_some_and(|only| only != peer) {
            continue;
        }
        let granted = disclosure.as_ref().filter(|disclosure| {
            disclosure.grants(
                crate::runtime::profile::ProfileScope::Personal,
                &disclosure.product_id,
                &peer,
            )
        });
        let message = ProfileReference {
            revision,
            discloser_product_id: disclosure
                .as_ref()
                .map(|disclosure| disclosure.product_id.clone())
                .unwrap_or_default(),
            reference: granted.map(|disclosure| disclosure.reference.clone()),
        }
        .into_message();
        let sealed = pair.seal(&message).map_err(|error| error.to_string())?;
        let now = current_unix_secs();
        let expiry = now
            .saturating_add(LIFETIME_SECS)
            .saturating_mul(1u64 << 32)
            | (revision & 0xffff_ffff);
        let fields = statement_fields_from_v01(Statement {
            proof: None,
            decryption_key: None,
            expiry: Some(expiry),
            channel: Some(pair.channel(KIND_PROFILE_REFERENCE).map_err(|error| error.to_string())?),
            topics: vec![pair.topic()],
            data: Some(sealed),
        })?;
        let signed = sign_statement_fields(*secret, public, fields)?;
        statements.push((peer, signed.encode()));
    }
    Ok(statements)
}

/// Submit what `build` returns. Returns how many were sent; the first
/// refusal stops the rest, which the next change or bootstrap resends.
pub(crate) async fn publish(
    context: &NativeChatContext,
    only: Option<[u8; 32]>,
) -> Result<usize, String> {
    let mut sent = 0;
    for (_, statement) in build(context, only).await? {
        context
            .services
            .statement_store
            .submit(statement, "host channel")
            .await
            .map_err(|error| format!("host channel submit: {error:?}"))?;
        sent += 1;
    }
    Ok(sent)
}

/// A running receiver for one wallet on one network.
pub(crate) struct Receiver {
    session: Vec<u8>,
    abort: AbortHandle,
}

impl Drop for Receiver {
    fn drop(&mut self) {
        self.abort.abort();
    }
}

impl Receiver {
    /// Whether this receiver serves `context`'s session.
    pub(crate) fn serves(&self, context: &NativeChatContext) -> bool {
        self.session == context.session.validation_id
    }

    /// Start receiving for `context`; the task ends when the session does.
    pub(crate) fn start(context: NativeChatContext) -> Self {
        let (abort, registration) = AbortHandle::new_pair();
        let session = context.session.validation_id.clone();
        let spawner = context.services.spawner.clone();
        spawner(Box::pin(async move {
            let _ = futures::future::Abortable::new(receive(context), registration).await;
        }));
        Self { session, abort }
    }
}

/// Watch every bootstrapped peer's topic, open what arrives, and record it
/// through the same path a Chat frame takes. Re-reads the peers every
/// `REFRESH`, resubscribing when they changed, until the session ends.
async fn receive(context: NativeChatContext) {
    loop {
        if context.require_current().is_err() {
            return;
        }
        let current_pairs = match pairs(&context).await {
            Ok(pairs) => pairs,
            Err(error) => {
                tracing::debug!(%error, "host channel peers unreadable");
                Delay::new(RETRY).await;
                continue;
            }
        };
        let by_topic: HashMap<[u8; 32], ([u8; 32], Pair)> = current_pairs
            .into_iter()
            .take(MAX_TOPICS)
            .map(|(peer, pair)| (pair.topic(), (peer, pair)))
            .collect();
        let topics: Vec<[u8; 32]> = by_topic.keys().copied().collect();
        if topics.is_empty() {
            Delay::new(REFRESH).await;
            continue;
        }
        let client = match context.services.statement_store.client("host channel").await {
            Ok(client) => client,
            Err(error) => {
                tracing::debug!(%error, "host channel store unavailable");
                Delay::new(RETRY).await;
                continue;
            }
        };
        let mut subscription = match statement_store_rpc::subscribe(
            &client,
            crate::host_logic::statement_store::TopicFilterKind::MatchAny,
            &topics,
        )
        .await
        {
            Ok(subscription) => subscription.fuse(),
            Err(error) => {
                tracing::debug!(%error, "host channel subscribe failed");
                Delay::new(RETRY).await;
                continue;
            }
        };
        let watched = topics;
        let mut refresh = Delay::new(REFRESH).fuse();
        loop {
            futures::select! {
                item = subscription.next() => match item {
                    Some(Ok(value)) => handle_page(&context, &by_topic, &value).await,
                    Some(Err(error)) => {
                        tracing::debug!(%error, "host channel subscription failed");
                        break;
                    }
                    None => break,
                },
                () = refresh => {
                    if context.require_current().is_err() {
                        return;
                    }
                    let current: Vec<[u8; 32]> = pairs(&context)
                        .await
                        .unwrap_or_default()
                        .into_iter()
                        .take(MAX_TOPICS)
                        .map(|(_, pair)| pair.topic())
                        .collect();
                    let mut sorted = watched.clone();
                    sorted.sort_unstable();
                    let mut now = current;
                    now.sort_unstable();
                    if sorted != now {
                        break;
                    }
                    refresh = Delay::new(REFRESH).fuse();
                }
            }
        }
    }
}

async fn handle_page(
    context: &NativeChatContext,
    by_topic: &HashMap<[u8; 32], ([u8; 32], Pair)>,
    value: &serde_json::Value,
) {
    let page = match parse_new_statements_result("host-channel".to_string(), value) {
        Ok(page) => page,
        Err(error) => {
            tracing::debug!(%error, "host channel page unreadable");
            return;
        }
    };
    let owner = owner_of(context);
    let mut changed = false;
    for bytes in page.statements {
        let Ok(statement) = decode_signed_statement(&bytes) else {
            continue;
        };
        let Some((peer, pair)) = statement
            .topics
            .iter()
            .find_map(|topic| by_topic.get(topic))
        else {
            continue;
        };
        let Some(data) = statement.data.as_deref() else {
            continue;
        };
        // The seal is the authentication: what does not open is not the peer's.
        let Ok(message) = pair.open(data) else {
            continue;
        };
        let Some(Ok(reference)) = ProfileReference::from_message(&message) else {
            continue;
        };
        let now = current_unix_secs().saturating_mul(1000);
        let _gate = context.services.profile_state_gate.lock().await;
        match record_personal_received_reference(
            &*context.services.platform,
            owner,
            *peer,
            reference.discloser_product_id,
            now,
            reference.revision,
            reference.reference,
        )
        .await
        {
            Ok(kept) => changed |= kept,
            Err(error) => tracing::debug!(%error, "host channel reference not recorded"),
        }
    }
    if changed {
        context
            .services
            .contact_avatars
            .redraw_owner(owner, &context.services.spawner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::HostInfo;
    use truapi::latest::HostPlatform;
    use crate::runtime::authority::AuthoritySession;
    use crate::runtime::profile::{Disclosure, received_reference, write_disclosure};
    use crate::runtime::services::RuntimeServices;
    use crate::test_support::StubPlatform;
    use futures::executor::block_on;
    use std::sync::Arc;

    const GENESIS: [u8; 32] = [2; 32];
    const REFERENCE: &str = "seity-contacts:v1:aa";

    struct Host {
        context: NativeChatContext,
        platform: Arc<StubPlatform>,
        identity: [u8; 32],
    }

    /// A wallet with no RPC: the builder and the page handler need none.
    fn host(seed: u8) -> Host {
        let platform = Arc::new(StubPlatform {
            chain_connect_error: Some("host channel test has no RPC"),
            ..Default::default()
        });
        let identity = [seed; 32];
        let context = NativeChatContext {
            services: RuntimeServices::with_chat_platform(
                platform.clone(),
                HostInfo {
                    name: "host channel test".into(),
                    icon: None,
                    version: None,
                    platform: HostPlatform::Unknown,
                },
                GENESIS,
                [3; 32],
                [4; 32],
                Arc::new(|_future| {}),
                None,
                None,
            ),
            session: AuthoritySession {
                public_key: identity,
                identity_account_id: Some(identity),
                lite_username: None,
                full_username: None,
                validation_id: vec![seed],
            },
            entropy: zeroize::Zeroizing::new(vec![seed; 16]),
            session_valid: Arc::new(|| true),
            chat_session_granted: Arc::new(|_| false),
            network_suffix: "test".into(),
            genesis_hash: GENESIS,
            coinage_instance_id: None,
        };
        Host {
            context,
            platform,
            identity,
        }
    }

    /// Both hosts hold each other's bootstrap.
    async fn bootstrapped(a: &Host, b: &Host) {
        let a_key = ChannelKey::derive(&a.context.entropy, &GENESIS, &b.identity).public();
        let b_key = ChannelKey::derive(&b.context.entropy, &GENESIS, &a.identity).public();
        upsert_peer(a.platform.as_ref(), owner_of(&a.context), b.identity, b_key)
            .await
            .unwrap();
        upsert_peer(b.platform.as_ref(), owner_of(&b.context), a.identity, a_key)
            .await
            .unwrap();
    }

    async fn disclose(host: &Host, reference: &str, contacts: Vec<[u8; 32]>) {
        write_disclosure(
            host.platform.as_ref(),
            owner_of(&host.context),
            &Disclosure {
                product_id: "seity.dot".into(),
                reference: reference.into(),
                revision: 0,
                all_chat_apps: false,
                app_products: Vec::new(),
                contacts,
            },
        )
        .await
        .unwrap();
    }

    fn page(statements: &[Vec<u8>]) -> serde_json::Value {
        serde_json::json!({
            "event": "newStatements",
            "data": {
                "statements": statements.iter().map(|s| format!("0x{}", hex::encode(s))).collect::<Vec<_>>(),
            }
        })
    }

    async fn held(host: &Host, peer: [u8; 32]) -> Option<Option<String>> {
        received_reference(host.platform.as_ref(), owner_of(&host.context), "any.dot", &peer)
            .await
            .unwrap()
            .map(|held| held.reference)
    }

    async fn by_topic(host: &Host) -> HashMap<[u8; 32], ([u8; 32], Pair)> {
        pairs(&host.context)
            .await
            .unwrap()
            .into_iter()
            .map(|(peer, pair)| (pair.topic(), (peer, pair)))
            .collect()
    }

    #[test]
    fn a_share_reaches_a_bootstrapped_peer_and_only_the_newest_revision_counts() {
        block_on(async {
            let alice = host(0x0a);
            let bob = host(0x0b);
            let carol = host(0x0c);
            bootstrapped(&alice, &bob).await;
            // Carol is picked but never bootstrapped: nothing can reach her.
            disclose(&alice, REFERENCE, vec![bob.identity, carol.identity]).await;

            let first = build(&alice.context, None).await.unwrap();
            assert_eq!(first.len(), 1, "one statement, for the one bootstrapped peer");
            assert_eq!(first[0].0, bob.identity);
            let statement = decode_signed_statement(&first[0].1).unwrap();
            let pair = ChannelKey::derive(&bob.context.entropy, &GENESIS, &alice.identity)
                .pair(&ChannelKey::derive(&alice.context.entropy, &GENESIS, &bob.identity).public())
                .unwrap();
            assert_eq!(statement.topics, vec![pair.topic()]);
            assert_eq!(statement.channel, Some(pair.channel(KIND_PROFILE_REFERENCE).unwrap()));

            // Bob opens it and holds the reference; Carol's host holds nothing.
            let topics = by_topic(&bob).await;
            handle_page(&bob.context, &topics, &page(&[first[0].1.clone()])).await;
            assert_eq!(held(&bob, alice.identity).await, Some(Some(REFERENCE.into())));
            assert_eq!(held(&carol, alice.identity).await, None);

            // A newer disclosure replaces it; replaying the older one changes nothing.
            disclose(&alice, "seity-contacts:v1:bb", vec![bob.identity]).await;
            let second = build(&alice.context, None).await.unwrap();
            handle_page(&bob.context, &topics, &page(&[second[0].1.clone()])).await;
            assert_eq!(held(&bob, alice.identity).await, Some(Some("seity-contacts:v1:bb".into())));
            handle_page(&bob.context, &topics, &page(&[first[0].1.clone()])).await;
            assert_eq!(held(&bob, alice.identity).await, Some(Some("seity-contacts:v1:bb".into())));

            // Unpicking Bob sends him a withdrawal.
            disclose(&alice, "seity-contacts:v1:bb", vec![]).await;
            let third = build(&alice.context, None).await.unwrap();
            assert_eq!(third.len(), 1);
            handle_page(&bob.context, &topics, &page(&[third[0].1.clone()])).await;
            assert_eq!(held(&bob, alice.identity).await, Some(None));
        });
    }

    #[test]
    fn what_the_pair_did_not_seal_is_dropped() {
        block_on(async {
            let alice = host(0x0a);
            let bob = host(0x0b);
            let mallory = host(0x0d);
            bootstrapped(&alice, &bob).await;
            bootstrapped(&mallory, &bob).await;
            disclose(&alice, REFERENCE, vec![bob.identity]).await;
            let topics = by_topic(&bob).await;

            // Tampered ciphertext on the right topic.
            let (_, statement) = build(&alice.context, None).await.unwrap().remove(0);
            let mut tampered = decode_signed_statement(&statement).unwrap();
            let data = tampered.data.as_mut().unwrap();
            let last = data.len() - 1;
            data[last] ^= 1;
            let fields = statement_fields_from_v01(Statement {
                proof: None,
                decryption_key: None,
                expiry: tampered.expiry,
                channel: tampered.channel,
                topics: tampered.topics.clone(),
                data: tampered.data.clone(),
            })
            .unwrap();
            let signer = derive_identity_keypair(&mallory.context.entropy, "test").unwrap();
            let resigned =
                sign_statement_fields(signer.secret.to_bytes(), signer.public.to_bytes(), fields)
                    .unwrap()
                    .encode();
            handle_page(&bob.context, &topics, &page(&[resigned])).await;
            assert_eq!(held(&bob, alice.identity).await, None);

            // Mallory's own valid share lands under Mallory, never under Alice.
            disclose(&mallory, "seity-contacts:v1:ee", vec![bob.identity]).await;
            let (_, from_mallory) = build(&mallory.context, None).await.unwrap().remove(0);
            handle_page(&bob.context, &topics, &page(&[from_mallory])).await;
            assert_eq!(held(&bob, alice.identity).await, None);
            assert_eq!(held(&bob, mallory.identity).await, Some(Some("seity-contacts:v1:ee".into())));
        });
    }

    #[test]
    fn only_targets_one_peer_and_nothing_is_built_before_a_disclosure() {
        block_on(async {
            let alice = host(0x0a);
            let bob = host(0x0b);
            let dave = host(0x0e);
            bootstrapped(&alice, &bob).await;
            bootstrapped(&alice, &dave).await;
            assert!(build(&alice.context, None).await.unwrap().is_empty());
            disclose(&alice, REFERENCE, vec![bob.identity, dave.identity]).await;
            assert_eq!(build(&alice.context, None).await.unwrap().len(), 2);
            let only = build(&alice.context, Some(dave.identity)).await.unwrap();
            assert_eq!(only.len(), 1);
            assert_eq!(only[0].0, dave.identity);
        });
    }
}
