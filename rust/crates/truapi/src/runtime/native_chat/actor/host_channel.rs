//! The host-channel bootstrap over Chat: this host's pairwise key to each
//! ready peer, once per session, on the same sealed path as a profile frame
//! (seity `doc/HOST_CHANNEL_PLAN.md`, steps 3 and 4).
//!
//! Prototype trims: no acknowledgement tracking and no resend. A frame the
//! peer never acknowledges lapses out of the outbox like a profile frame.
//! The receiving side stores the peer's key; sending on the channel itself
//! is the runtime's job, not the actor's.

use super::*;
use crate::host_logic::host_channel::ChannelKey;
use crate::runtime::chat_device::HostChannelBootstrapFrame;
use crate::runtime::native_chat::background::require_authorized;

impl NativeChatActor {
	/// Queue one bootstrap frame for every ready peer this session has not
	/// bootstrapped yet, as far as the outbox has room. `true` when anything
	/// was queued.
	pub(in crate::runtime::native_chat) async fn publish_host_channel_bootstraps(
		self: &Arc<Self>,
		context: &NativeChatContext,
	) -> Result<bool, Error> {
		context.require_current()?;
		// The guard is not held across the read: the set is a session memo.
		let sent = self
			.host_channel_bootstrapped
			.lock()
			.expect("host channel bootstrap set poisoned")
			.clone();
		let pending: Vec<[u8; 32]> = self
			.store
			.read(move |state| {
				state
					.peers
					.iter()
					.filter(|peer| peer.ready() && !sent.contains(&peer.identity))
					.map(|peer| peer.identity)
					.collect()
			})
			.await?;
		if pending.is_empty() {
			return Ok(false);
		}
		tracing::debug!(
			product = %self.product,
			pending = ?pending.iter().map(|peer| hex::encode(&peer[..4])).collect::<Vec<_>>(),
			"host channel bootstrap: ready peers not yet sent this session"
		);
		require_authorized(context, &self.product).await?;
		let keys: Vec<([u8; 32], [u8; 32])> = pending
			.iter()
			.map(|identity| {
				let key = ChannelKey::derive(&context.entropy, &context.genesis_hash, identity);
				(*identity, key.public())
			})
			.collect();
		let actor = self.clone();
		let valid = context.session_valid.clone();
		let queued: Vec<[u8; 32]> = self
			.store
			.update(move |state| {
				if !valid() {
					return Err(Error::NotConnected);
				}
				let now = current_unix_secs().saturating_mul(1000);
				let mut queued = Vec::new();
				for (identity, public) in keys {
					let peer = state.peer(&identity)?.clone();
					if !peer.ready() {
						continue;
					}
					let tag = hash(&Zeroizing::new(
						(b"native-chat-host-channel-tag-v1", &state.secret.0, identity, now)
							.encode(),
					));
					let request_id = format!("hostchan-{}", hex::encode(&tag[..8]));
					let bytes =
						wire::encode_host_channel_bootstrap_message(&request_id, now, &public)
							.map_err(|_| Error::InvalidRequest)?;
					let messages = Zeroizing::new(vec![bytes]);
					let statement = actor.multi_statement(
						state,
						&peer,
						&peer.active_devices(),
						&request_id,
						&messages,
					)?;
					// One pending bootstrap per peer.
					state.outbox.retain(|entry| {
						entry.peer != identity || entry.kind != OutgoingKind::HostChannelBootstrap
					});
					match state.queue(Outgoing {
						peer: identity,
						request_id,
						digest: hash(&messages.encode()),
						kind: OutgoingKind::HostChannelBootstrap,
						roster_revision: peer.revision,
						statement,
						last_attempt: 0,
					}) {
						Ok(()) => queued.push(identity),
						// No room: the rest wait for a later publish.
						Err(Error::StorageUnavailable) => break,
						Err(error) => return Err(error),
					}
				}
				Ok(queued)
			})
			.await?;
		tracing::debug!(
			queued = ?queued.iter().map(|peer| hex::encode(&peer[..4])).collect::<Vec<_>>(),
			"host channel bootstrap: queued in the Chat outbox"
		);
		let any = !queued.is_empty();
		self.host_channel_bootstrapped
			.lock()
			.expect("host channel bootstrap set poisoned")
			.extend(queued);
		Ok(any)
	}

	/// Store the keys `peer` sent. The newest frame wins; an unchanged key
	/// changes nothing.
	pub(super) async fn record_host_channel_bootstraps(
		&self,
		context: &NativeChatContext,
		peer: [u8; 32],
		frames: Vec<HostChannelBootstrapFrame>,
	) -> Result<(), Error> {
		let Some(newest) = frames.into_iter().max_by_key(|frame| frame.timestamp) else {
			return Ok(());
		};
		tracing::debug!(
			peer = %hex::encode(&peer[..4]),
			"host channel bootstrap: received a contact's channel key"
		);
		let owner = super::profile::profile_owner(context);
		let changed = crate::runtime::host_channel::upsert_peer(
			&*context.services.platform,
			owner,
			peer,
			newest.channel_public_key,
		)
		.await
		.map_err(|_| Error::StorageUnavailable)?;
		if changed {
			// The peer can be reached now: send it what the user discloses.
			let context = context.clone();
			let spawner = context.services.spawner.clone();
			spawner(Box::pin(async move {
				match crate::runtime::host_channel::publish(&context, Some(peer)).await {
					Ok(sent) => tracing::debug!(sent, "host channel published to a new peer"),
					Err(error) => tracing::debug!(%error, "host channel publish deferred"),
				}
			}));
		}
		Ok(())
	}
}
