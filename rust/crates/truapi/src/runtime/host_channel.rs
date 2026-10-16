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
			},
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
			},
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
			},
		};
		tracing::debug!(topics = topics.len(), "host channel watching bootstrapped peers");
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
		},
	};
	let owner = owner_of(context);
	let mut changed = false;
	for bytes in page.statements {
		let Ok(statement) = decode_signed_statement(&bytes) else {
			continue;
		};
		let Some((peer, pair)) = statement.topics.iter().find_map(|topic| by_topic.get(topic))
		else {
			continue;
		};
		let Some(data) = statement.data.as_deref() else {
			continue;
		};
		// The seal is the authentication: what does not open is not the peer's.
		let Ok(message) = pair.open(data) else {
			tracing::debug!(peer = %hex::encode(&peer[..4]), "host channel statement did not open");
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
			Ok(kept) => {
				if kept {
					tracing::info!(
						peer = %hex::encode(&peer[..4]),
						"host channel reference recorded from a contact"
					);
				}
				changed |= kept;
			},
			Err(error) => tracing::debug!(%error, "host channel reference not recorded"),
		}
	}
	if changed {
		context.services.contact_avatars.redraw_owner(owner, &context.services.spawner);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		platform::HostInfo,
		runtime::{
			authority::AuthoritySession,
			profile::{Disclosure, received_reference, write_disclosure},
			services::RuntimeServices,
		},
		test_support::{StubPlatform, new_statements_frame, subscribe_ack_frame},
	};
	use futures::{
		executor::{LocalPool, block_on},
		task::LocalSpawnExt,
	};
	use parking_lot::Mutex;
	use std::sync::Arc;
	use truapi::latest::HostPlatform;

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
		host_on(seed, platform, Arc::new(|_future| {}), GENESIS)
	}

	fn host_on(
		seed: u8,
		platform: Arc<StubPlatform>,
		spawner: crate::subscription::Spawner,
		genesis: [u8; 32],
	) -> Host {
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
				genesis,
				[3; 32],
				[4; 32],
				spawner,
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
			genesis_hash: genesis,
			coinage_instance_id: None,
		};
		Host { context, platform, identity }
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

	/// Drive real RPC tasks to quiescence, without threads, sleeps or polling deadlines.
	#[derive(Default)]
	struct RpcExecutor {
		pool: LocalPool,
		tasks: Arc<Mutex<Vec<futures::future::BoxFuture<'static, ()>>>>,
	}

	impl RpcExecutor {
		fn spawner(&self) -> crate::subscription::Spawner {
			let tasks = self.tasks.clone();
			Arc::new(move |task| tasks.lock().push(task))
		}

		fn settle(&mut self) {
			loop {
				let tasks = std::mem::take(&mut *self.tasks.lock());
				for task in tasks {
					self.pool.spawner().spawn_local(task).unwrap();
				}
				self.pool.run_until_stalled();
				if self.tasks.lock().is_empty() {
					break;
				}
			}
		}

		fn complete<F>(&mut self, future: F) -> F::Output
		where
			F: core::future::Future + 'static,
			F::Output: 'static,
		{
			let result = self.pool.spawner().spawn_local_with_handle(future).unwrap();
			self.settle();
			result.now_or_never().expect("RPC completed without a timer or external wake")
		}
	}

	/// Keep only persisted storage across a host reopen, never a cached pair or receiver.
	fn reopen(
		host: &Host,
		executor: &RpcExecutor,
		responses: Vec<String>,
		genesis: [u8; 32],
	) -> Host {
		host_on(
			host.identity[0],
			Arc::new(StubPlatform {
				local_storage: host.platform.local_storage.clone(),
				rpc_responses: responses,
				..Default::default()
			}),
			executor.spawner(),
			genesis,
		)
	}

	/// Capture actual publish RPC bytes; no builder shortcut or Chat actor is involved.
	fn published(host: &Host, only: [u8; 32]) -> Vec<u8> {
		let mut executor = RpcExecutor::default();
		let sender = reopen(
			host,
			&executor,
			vec![r#"{"jsonrpc":"2.0","id":"truapi:1","result":{"status":"new"}}"#.into()],
			host.context.genesis_hash,
		);
		let context = sender.context.clone();
		assert_eq!(executor.complete(async move { publish(&context, Some(only)).await }).unwrap(), 1);
		let requests = sender.platform.sent_rpc.lock().unwrap();
		assert_eq!(requests.len(), 1, "only direct host-channel publication is allowed");
		let request: serde_json::Value = serde_json::from_str(&requests[0]).unwrap();
		assert_eq!(request["method"], "statement_submit");
		assert_eq!(*sender.platform.chain_connects.lock().unwrap(), vec![host.context.genesis_hash]);
		hex::decode(request["params"][0].as_str().unwrap().strip_prefix("0x").unwrap()).unwrap()
	}

	/// Feed captured publish bytes through the real subscription, AEAD opener and recorder.
	fn receive_after_reopen(host: &Host, statements: Vec<Vec<u8>>) -> Host {
		let mut executor = RpcExecutor::default();
		let receiver_host = reopen(
			host,
			&executor,
			vec![
				subscribe_ack_frame("truapi:1", "direct-profile"),
				new_statements_frame("direct-profile", statements),
			],
			host.context.genesis_hash,
		);
		let expected_topics = block_on(by_topic(&receiver_host));
		assert!(!expected_topics.is_empty(), "persisted bootstrap must survive reopening");
		assert_eq!(
			block_on(read_peers(receiver_host.platform.as_ref(), owner_of(&receiver_host.context))).unwrap(),
			block_on(read_peers(host.platform.as_ref(), owner_of(&host.context))).unwrap(),
		);
		let receiver = Receiver::start(receiver_host.context.clone());
		executor.settle();
		{
			let requests = receiver_host.platform.sent_rpc.lock().unwrap();
			assert_eq!(requests.len(), 1, "receiver must not fall back to Chat");
			let request: serde_json::Value = serde_json::from_str(&requests[0]).unwrap();
			assert_eq!(request["method"], "statement_subscribeStatement");
			let mut watched: Vec<String> = request["params"][0]["matchAny"]
				.as_array().unwrap().iter().map(|value| value.as_str().unwrap().to_owned()).collect();
			watched.sort_unstable();
			let mut expected: Vec<String> = expected_topics.keys()
				.map(|topic| format!("0x{}", hex::encode(topic))).collect();
			expected.sort_unstable();
			assert_eq!(watched, expected);
		}
		assert_eq!(
			*receiver_host.platform.chain_connects.lock().unwrap(),
			vec![host.context.genesis_hash],
		);
		drop(receiver);
		executor.settle();
		receiver_host
	}

	fn assert_direct_reference(host: &Host, peer: [u8; 32], expected: Option<&str>) {
		let received = block_on(received_reference(
			host.platform.as_ref(), owner_of(&host.context), "any.dot", &peer,
		)).unwrap().expect("the direct receiver recorded a reference or durable withdrawal");
		assert_eq!(received.peer_identity, peer);
		assert_eq!(received.discloser_product_id, "seity.dot");
		assert_eq!(received.reference.as_deref(), expected);
	}

	#[test]
	fn direct_publish_receiver_updates_withdraws_and_rejects_replay_after_reopen() {
		let alice = host(0x0a);
		let bob = host(0x0b);
		block_on(bootstrapped(&alice, &bob));
		// Bootstrap is the only Chat prerequisite. Neither host has a Chat session grant,
		// actor or product relay for any of the following deliveries.
		block_on(disclose(&alice, REFERENCE, vec![bob.identity]));
		let first = published(&alice, bob.identity);
		let bob = receive_after_reopen(&bob, vec![first.clone()]);
		assert_direct_reference(&bob, alice.identity, Some(REFERENCE));

		block_on(disclose(&alice, "seity-contacts:v1:bb", vec![bob.identity]));
		let second = published(&alice, bob.identity);
		let bob = receive_after_reopen(&bob, vec![second.clone()]);
		assert_direct_reference(&bob, alice.identity, Some("seity-contacts:v1:bb"));

		block_on(disclose(&alice, "seity-contacts:v1:bb", vec![]));
		let withdrawal = published(&alice, bob.identity);
		let bob = receive_after_reopen(&bob, vec![withdrawal]);
		assert_direct_reference(&bob, alice.identity, None);

		// Reopen again: a durable revision fence, not an in-memory dedupe, must win.
		let bob = receive_after_reopen(&bob, vec![second, first]);
		assert_direct_reference(&bob, alice.identity, None);
	}

	/// Preserve a real signature while putting a different pair's ciphertext on the
	/// watched topic. Topic routing alone must not authorize a reference.
	fn retarget(host: &Host, bytes: &[u8], topic: [u8; 32]) -> Vec<u8> {
		let statement = decode_signed_statement(bytes).unwrap();
		let fields = statement_fields_from_v01(Statement {
			proof: None,
			decryption_key: None,
			expiry: statement.expiry,
			channel: statement.channel,
			topics: vec![topic],
			data: statement.data,
		}).unwrap();
		let signer = derive_identity_keypair(&host.context.entropy, &host.context.network_suffix).unwrap();
		sign_statement_fields(signer.secret.to_bytes(), signer.public.to_bytes(), fields).unwrap().encode()
	}

	#[test]
	fn direct_receiver_rejects_a_different_peers_ciphertext_on_its_watched_topic() {
		let alice = host(0x0a);
		let bob = host(0x0b);
		let carol = host(0x0c);
		block_on(bootstrapped(&alice, &bob));
		block_on(bootstrapped(&alice, &carol));
		block_on(disclose(&alice, REFERENCE, vec![carol.identity]));
		let for_carol = published(&alice, carol.identity);
		let topic = *block_on(by_topic(&bob)).keys().next().unwrap();
		let bob = receive_after_reopen(&bob, vec![retarget(&alice, &for_carol, topic)]);
		assert_eq!(block_on(held(&bob, alice.identity)), None);
		// Positive control: this same receiver accepts Alice's correctly paired share.
		block_on(disclose(&alice, REFERENCE, vec![bob.identity]));
		let bob = receive_after_reopen(&bob, vec![published(&alice, bob.identity)]);
		assert_direct_reference(&bob, alice.identity, Some(REFERENCE));
	}

	#[test]
	fn direct_receiver_isolates_persisted_peers_and_ciphertext_by_network() {
		let alice = host(0x0a);
		let bob = host(0x0b);
		block_on(bootstrapped(&alice, &bob));
		block_on(disclose(&alice, REFERENCE, vec![bob.identity]));
		let original = published(&alice, bob.identity);
		let executor = RpcExecutor::default();
		let other_network = reopen(&bob, &executor, vec![], [0x77; 32]);
		assert!(block_on(read_peers(
			other_network.platform.as_ref(), owner_of(&other_network.context),
		)).unwrap().is_empty(), "another network must not inherit the bootstrap roster");

		// Even copying the public bootstrap key cannot reuse a pair across networks.
		let peers = block_on(read_peers(bob.platform.as_ref(), owner_of(&bob.context))).unwrap();
		block_on(upsert_peer(
			other_network.platform.as_ref(), owner_of(&other_network.context),
			alice.identity, peers[0].channel_public_key,
		)).unwrap();
		let topic = *block_on(by_topic(&other_network)).keys().next().unwrap();
		let other_network = receive_after_reopen(
			&other_network, vec![retarget(&alice, &original, topic)],
		);
		assert_eq!(block_on(held(&other_network, alice.identity)), None);
		let bob = receive_after_reopen(&bob, vec![original]);
		assert_direct_reference(&bob, alice.identity, Some(REFERENCE));
		assert_eq!(block_on(held(&other_network, alice.identity)), None);
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
