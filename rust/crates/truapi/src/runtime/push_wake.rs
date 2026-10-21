//! Push wakes: the statement-store topics that wake the worker of a product.
//!
//! A worker declares its wake watches with `Worker::set_wake_watches`. The
//! core keeps the watches of every product in core storage, so they stay
//! while the worker is stopped and after the app restarts. The core gives the
//! topics of all products, with a key of this device, to the host's push
//! relay ([`WakeRelay`]). The product never sees the relay or the key.
//!
//! The relay sends a wake push for a new statement on a watched topic: the
//! topic, the hash of the statement, the send time and a MAC with the key.
//! The push carries no statement content. The core checks the push
//! ([`RuntimeServices::handle_wake_push`]) and then wakes each product that
//! watches the topic as a notification wake (30 s, at most 12 in one hour).

use std::{
	collections::{BTreeMap, BTreeSet, VecDeque},
	sync::{Arc, Mutex},
};

use async_trait::async_trait;
use futures::FutureExt;
use parity_scale_codec::{Decode, Encode};
use serde_json::Value;
use tracing::{debug, warn};
use truapi::latest;

use crate::{
	Bytes32,
	platform::{CoreStorageKey, normalize_chat_identifier},
	runtime::{ChatWakeOutcome, ChatWakeReason, RuntimeServices},
	unix_time::current_unix_millis,
};

/// Largest number of wake watches of one product.
pub const MAX_WAKE_WATCHES: usize = 64;

/// Largest number of topics of one wake watch.
pub const MAX_WAKE_WATCH_TOPICS: usize = 64;

/// Largest number of distinct topics of all wake watches of one product.
pub const MAX_WAKE_TOPICS: usize = 256;

/// Oldest wake push the core accepts, in milliseconds after its send time.
pub const WAKE_PUSH_MAX_AGE_MS: u64 = 15 * 60 * 1000;

/// Largest time a wake push may have been sent after the clock of the device,
/// in milliseconds.
pub const WAKE_PUSH_MAX_SKEW_MS: u64 = 5 * 60 * 1000;

/// The version of the wake push format.
pub const WAKE_PUSH_VERSION: u64 = 1;

/// The key of the wake push in the push payload.
pub const WAKE_PUSH_PAYLOAD_KEY: &str = "truapi-wake";

/// Domain of the wake push MAC.
const MAC_DOMAIN: &[u8] = b"truapi/wake-push/v1";

/// Number of statement hashes the core remembers to refuse a second push.
const RECENT_PUSHES: usize = 512;

/// Longest wait for the relay to answer a registration.
const RELAY_TIMEOUT: core::time::Duration = core::time::Duration::from_secs(15);

/// Version byte in front of the stored registry.
const REGISTRY_VERSION: u8 = 1;

/// What the host gives to its push relay: the key of this device and the
/// topics of every product.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct WakeRelayRegistration {
	/// The key the relay signs each wake push of this device with. The core
	/// makes it and keeps it in core storage.
	pub key: Bytes32,
	/// The watched topics of all products, sorted, each once. An empty list
	/// removes the registration of the device.
	pub topics: Vec<Bytes32>,
}

/// The push relay client of a host.
///
/// The host adds what identifies the device to the relay (a push token and
/// the app id) and sends the registration to its relay endpoint. Only the
/// core calls it; no product reaches it.
#[async_trait]
pub trait WakeRelay: Send + Sync {
	/// Replace the registration of this device with `registration`.
	async fn register_wake_topics(
		&self,
		registration: WakeRelayRegistration,
	) -> Result<(), latest::GenericError>;
}

/// A wake push as the relay sends it, under [`WAKE_PUSH_PAYLOAD_KEY`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WakePush {
	/// The watched topic of the new statement.
	pub topic: Bytes32,
	/// [`statement_hash`] of the new statement.
	pub statement_hash: Bytes32,
	/// When the relay sent the push, in milliseconds since the Unix epoch.
	pub sent_at_ms: u64,
	/// [`wake_push_mac`] of the three fields above.
	pub mac: Bytes32,
}

impl WakePush {
	/// A push for `topic` and `statement_hash` sent at `sent_at_ms`, signed
	/// with `key`.
	pub fn sign(key: &Bytes32, topic: Bytes32, statement_hash: Bytes32, sent_at_ms: u64) -> Self {
		let mac = wake_push_mac(key, &topic, &statement_hash, sent_at_ms);
		Self { topic, statement_hash, sent_at_ms, mac }
	}

	/// The JSON object of the push, the value of [`WAKE_PUSH_PAYLOAD_KEY`].
	pub fn to_json(&self) -> Value {
		serde_json::json!({
			"v": WAKE_PUSH_VERSION,
			"topic": format!("0x{}", hex::encode(self.topic)),
			"statement": format!("0x{}", hex::encode(self.statement_hash)),
			"sentAt": self.sent_at_ms,
			"mac": format!("0x{}", hex::encode(self.mac)),
		})
	}

	/// Read a push from the text of its JSON object.
	pub fn parse(payload: &str) -> Result<Self, PushWakeRejection> {
		let value: Value = serde_json::from_str(payload)
			.map_err(|error| malformed(format!("not JSON: {error}")))?;
		match value.get("v").and_then(Value::as_u64) {
			Some(WAKE_PUSH_VERSION) => {},
			Some(version) => return Err(malformed(format!("unknown version {version}"))),
			None => return Err(malformed("no version".to_string())),
		}
		let sent_at_ms = value
			.get("sentAt")
			.and_then(Value::as_u64)
			.ok_or_else(|| malformed("no sentAt".to_string()))?;
		Ok(Self {
			topic: hex_field(&value, "topic")?,
			statement_hash: hex_field(&value, "statement")?,
			sent_at_ms,
			mac: hex_field(&value, "mac")?,
		})
	}
}

/// The statement hash of a wake push: BLAKE2b-256 of the SCALE-encoded
/// statement as the relay received it. The core uses it only to refuse a
/// second push for the same statement.
pub fn statement_hash(statement: &[u8]) -> Bytes32 {
	let mut hash = [0u8; 32];
	hash.copy_from_slice(blake2b_simd::Params::new().hash_length(32).hash(statement).as_bytes());
	hash
}

/// The MAC of a wake push: keyed BLAKE2b-256 over the domain, the topic, the
/// statement hash and the send time (little endian).
pub fn wake_push_mac(
	key: &Bytes32,
	topic: &Bytes32,
	statement_hash: &Bytes32,
	sent_at_ms: u64,
) -> Bytes32 {
	let mut state = blake2b_simd::Params::new().hash_length(32).key(key).to_state();
	state.update(MAC_DOMAIN);
	state.update(topic);
	state.update(statement_hash);
	state.update(&sent_at_ms.to_le_bytes());
	let mut mac = [0u8; 32];
	mac.copy_from_slice(state.finalize().as_bytes());
	mac
}

/// Why the core refused a wake push. The core wakes nothing then.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Error))]
pub enum PushWakeRejection {
	/// The payload is not a wake push.
	#[error("malformed wake push: {reason}")]
	Malformed {
		/// What is wrong with it.
		reason: String,
	},
	/// No product of this device has wake watches, so no key exists.
	#[error("this device has no wake registration")]
	NotRegistered,
	/// The MAC does not match the key of this device.
	#[error("the wake push has a bad signature")]
	BadSignature,
	/// The push is older than 15 minutes, or from more than 5 minutes ahead.
	#[error("the wake push is stale")]
	Stale,
	/// A push for the same statement came before.
	#[error("the wake push came before")]
	Replayed,
	/// No product watches the topic of the push.
	#[error("no product watches the topic of the wake push")]
	UnknownTopic,
	/// The core could not read its wake registration.
	#[error("{reason}")]
	Storage {
		/// Human-readable failure reason.
		reason: String,
	},
}

/// What a push wake did for one product.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum PushWakeOutcome {
	/// The core holds the worker until this time, in milliseconds since the
	/// Unix epoch.
	Held {
		/// The end of the hold.
		until_ms: u64,
	},
	/// The product used all its notification wakes of this hour.
	BudgetSpent,
	/// The app is in the foreground and the product declares a foreground
	/// worker, so the worker runs and reads the statement itself. No budget
	/// goes.
	Running,
}

/// One product that a push woke.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct PushWakeDelivery {
	/// The product.
	pub product_id: String,
	/// The room of the watch.
	pub room_id: String,
	/// The watch that has the topic.
	pub watch_id: String,
	/// What the wake did.
	pub outcome: PushWakeOutcome,
}

/// The wake watches of every product, the relay of the host, and the pushes
/// that came.
#[derive(Default)]
pub struct PushWakes {
	/// The registry, read from core storage at first use.
	registry: futures::lock::Mutex<Option<Registry>>,
	/// The push relay of the host, when it has one.
	relay: Mutex<Option<Arc<dyn WakeRelay>>>,
	/// Statement hashes of the last accepted pushes, oldest first.
	recent: Mutex<VecDeque<Bytes32>>,
}

/// The stored wake watches of all products and the key of this device.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
struct Registry {
	/// The key the relay signs wake pushes with.
	key: Bytes32,
	/// The watches of each product.
	products: BTreeMap<String, Vec<latest::WorkerWakeWatch>>,
}

impl Registry {
	fn new() -> Result<Self, String> {
		let mut key = [0u8; 32];
		getrandom::getrandom(&mut key).map_err(|error| format!("no random key: {error}"))?;
		Ok(Self { key, products: BTreeMap::new() })
	}

	fn registration(&self) -> WakeRelayRegistration {
		let topics: BTreeSet<Bytes32> = self
			.products
			.values()
			.flatten()
			.flat_map(|watch| watch.topics.iter().copied())
			.collect();
		WakeRelayRegistration { key: self.key, topics: topics.into_iter().collect() }
	}

	/// Each product and watch that has `topic`.
	fn watchers(&self, topic: &Bytes32) -> Vec<(String, latest::WorkerWakeWatch)> {
		self.products
			.iter()
			.flat_map(|(product_id, watches)| {
				watches
					.iter()
					.filter(|watch| watch.topics.contains(topic))
					.map(|watch| (product_id.clone(), watch.clone()))
			})
			.collect()
	}

	fn encode_stored(&self) -> Vec<u8> {
		let mut stored = vec![REGISTRY_VERSION];
		self.encode_to(&mut stored);
		stored
	}

	fn decode_stored(stored: &[u8]) -> Result<Self, String> {
		match stored.split_first() {
			Some((&REGISTRY_VERSION, mut rest)) => Self::decode(&mut rest)
				.map_err(|error| format!("the stored wake registry is damaged: {error}")),
			Some((version, _)) => Err(format!("unknown wake registry version {version}")),
			None => Err("the stored wake registry is empty".to_string()),
		}
	}
}

/// Check the watches of one product and make their ids and topics canonical.
fn validate_watches(
	watches: Vec<latest::WorkerWakeWatch>,
) -> Result<Vec<latest::WorkerWakeWatch>, latest::HostWorkerSetWakeWatchesError> {
	use latest::HostWorkerSetWakeWatchesError as Error;
	if watches.len() > MAX_WAKE_WATCHES {
		return Err(Error::TooMany {
			reason: format!("{} watches, at most {MAX_WAKE_WATCHES}", watches.len()),
		});
	}
	let mut ids = BTreeSet::new();
	let mut all_topics = BTreeSet::new();
	let mut checked = Vec::with_capacity(watches.len());
	for watch in watches {
		let invalid = |error: crate::platform::ChatFieldError| Error::InvalidWatch {
			reason: error.to_string(),
		};
		let id = normalize_chat_identifier("id", &watch.id).map_err(invalid)?;
		let room_id = normalize_chat_identifier("roomId", &watch.room_id).map_err(invalid)?;
		if !ids.insert(id.clone()) {
			return Err(Error::InvalidWatch { reason: format!("two watches have the id {id}") });
		}
		let topics: BTreeSet<Bytes32> = watch.topics.into_iter().collect();
		if topics.is_empty() {
			return Err(Error::InvalidWatch { reason: format!("watch {id} has no topic") });
		}
		if topics.len() > MAX_WAKE_WATCH_TOPICS {
			return Err(Error::TooMany {
				reason: format!(
					"watch {id} has {} topics, at most {MAX_WAKE_WATCH_TOPICS}",
					topics.len()
				),
			});
		}
		all_topics.extend(topics.iter().copied());
		checked.push(latest::WorkerWakeWatch { id, room_id, topics: topics.into_iter().collect() });
	}
	if all_topics.len() > MAX_WAKE_TOPICS {
		return Err(Error::TooMany {
			reason: format!("{} topics, at most {MAX_WAKE_TOPICS}", all_topics.len()),
		});
	}
	Ok(checked)
}

fn malformed(reason: String) -> PushWakeRejection {
	PushWakeRejection::Malformed { reason }
}

fn hex_field(value: &Value, field: &str) -> Result<Bytes32, PushWakeRejection> {
	let text = value
		.get(field)
		.and_then(Value::as_str)
		.ok_or_else(|| malformed(format!("no {field}")))?;
	let bytes = hex::decode(text.strip_prefix("0x").unwrap_or(text))
		.map_err(|error| malformed(format!("{field} is not hex: {error}")))?;
	bytes.try_into().map_err(|_| malformed(format!("{field} is not 32 bytes")))
}

impl RuntimeServices {
	/// Replace the wake watches of `product_id` and give the topics of all
	/// products to the push relay.
	pub async fn set_wake_watches(
		&self,
		product_id: &str,
		watches: Vec<latest::WorkerWakeWatch>,
	) -> Result<latest::WorkerWakeDelivery, latest::HostWorkerSetWakeWatchesError> {
		let watches = validate_watches(watches)?;
		let unknown = |reason: String| latest::HostWorkerSetWakeWatchesError::Unknown { reason };
		let registration = {
			let mut guard = self.push_wakes.registry.lock().await;
			let registry = self.loaded_registry(&mut guard).await.map_err(unknown)?;
			let mut changed = registry.clone();
			if watches.is_empty() {
				changed.products.remove(product_id);
			} else {
				changed.products.insert(product_id.to_string(), watches);
			}
			if changed != *registry {
				self.platform
					.write_core_storage(CoreStorageKey::PushWakeRegistry, changed.encode_stored())
					.await
					.map_err(|error| unknown(error.reason))?;
				*registry = changed;
			}
			registry.registration()
		};
		let Some(relay) = self.push_wakes.relay() else {
			return Ok(latest::WorkerWakeDelivery::Unavailable {
				reason: "this host has no push relay".to_string(),
			});
		};
		Ok(match register(relay, registration).await {
			Ok(()) => latest::WorkerWakeDelivery::Push,
			Err(reason) => {
				warn!(product_id, %reason, "the push relay refused the wake topics");
				latest::WorkerWakeDelivery::Unavailable { reason }
			},
		})
	}

	/// Install the push relay of the host, or remove it with `None`, and give
	/// it the stored topics of all products. An error says that the relay
	/// refused them; the relay stays installed.
	pub async fn set_wake_relay(&self, relay: Option<Arc<dyn WakeRelay>>) -> Result<(), String> {
		*self.push_wakes.relay.lock().expect("wake relay mutex poisoned") = relay.clone();
		let Some(relay) = relay else {
			return Ok(());
		};
		let registration = {
			let mut guard = self.push_wakes.registry.lock().await;
			match self.stored_registry(&mut guard).await? {
				Some(registry) => registry.registration(),
				None => return Ok(()),
			}
		};
		if registration.topics.is_empty() {
			return Ok(());
		}
		register(relay, registration).await
	}

	/// Check a wake push and wake each product that watches its topic. The
	/// payload is the text of the JSON object under [`WAKE_PUSH_PAYLOAD_KEY`].
	pub async fn handle_wake_push(
		self: &Arc<Self>,
		payload: &str,
	) -> Result<Vec<PushWakeDelivery>, PushWakeRejection> {
		self.handle_wake_push_at(payload, current_unix_millis(), |services, product_id, reason| {
			services.wake_product(product_id, reason)
		})
		.await
	}

	/// [`Self::handle_wake_push`] at `now_ms`, with `wake` holding the worker.
	async fn handle_wake_push_at(
		self: &Arc<Self>,
		payload: &str,
		now_ms: u64,
		wake: impl Fn(
			&Arc<Self>,
			&str,
			ChatWakeReason,
		) -> Result<ChatWakeOutcome, crate::platform::ChatFieldError>,
	) -> Result<Vec<PushWakeDelivery>, PushWakeRejection> {
		let push = WakePush::parse(payload)?;
		let watchers = {
			let mut guard = self.push_wakes.registry.lock().await;
			let registry = self
				.stored_registry(&mut guard)
				.await
				.map_err(|reason| PushWakeRejection::Storage { reason })?
				.ok_or(PushWakeRejection::NotRegistered)?;
			let expected =
				wake_push_mac(&registry.key, &push.topic, &push.statement_hash, push.sent_at_ms);
			if expected != push.mac {
				return Err(PushWakeRejection::BadSignature);
			}
			if now_ms.saturating_sub(push.sent_at_ms) > WAKE_PUSH_MAX_AGE_MS ||
				push.sent_at_ms.saturating_sub(now_ms) > WAKE_PUSH_MAX_SKEW_MS
			{
				return Err(PushWakeRejection::Stale);
			}
			registry.watchers(&push.topic)
		};
		if watchers.is_empty() {
			return Err(PushWakeRejection::UnknownTopic);
		}
		{
			let mut recent = self.push_wakes.recent.lock().expect("wake push mutex poisoned");
			if recent.contains(&push.statement_hash) {
				return Err(PushWakeRejection::Replayed);
			}
			if recent.len() == RECENT_PUSHES {
				recent.pop_front();
			}
			recent.push_back(push.statement_hash);
		}
		let mut deliveries = Vec::with_capacity(watchers.len());
		for (product_id, watch) in watchers {
			let outcome = if self.worker_ledger.app_foreground() &&
				self.worker_ledger.keep_alive(&product_id) == crate::WorkerKeepAlive::Foreground
			{
				PushWakeOutcome::Running
			} else {
				let reason = ChatWakeReason::Notification {
					room_id: watch.room_id.clone(),
					watch_id: watch.id.clone(),
				};
				match wake(self, &product_id, reason) {
					Ok(ChatWakeOutcome::Held { until_ms }) => PushWakeOutcome::Held { until_ms },
					Ok(ChatWakeOutcome::BudgetSpent) => PushWakeOutcome::BudgetSpent,
					Err(error) => {
						warn!(product_id, %error, "a stored wake watch names an invalid room");
						continue;
					},
				}
			};
			debug!(product_id, watch_id = %watch.id, ?outcome, "push wake");
			deliveries.push(PushWakeDelivery {
				product_id,
				room_id: watch.room_id,
				watch_id: watch.id,
				outcome,
			});
		}
		Ok(deliveries)
	}

	/// The registry under `guard`, read from core storage at first use, or a
	/// new one with a new key.
	async fn loaded_registry<'a>(
		&self,
		guard: &'a mut Option<Registry>,
	) -> Result<&'a mut Registry, String> {
		if self.stored_registry(guard).await?.is_none() {
			*guard = Some(Registry::new()?);
		}
		Ok(guard.as_mut().expect("the registry is loaded"))
	}

	/// The registry under `guard`, read from core storage at first use. `None`
	/// when no product ever set wake watches.
	async fn stored_registry<'a>(
		&self,
		guard: &'a mut Option<Registry>,
	) -> Result<Option<&'a mut Registry>, String> {
		if guard.is_none() {
			let stored = self
				.platform
				.read_core_storage(CoreStorageKey::PushWakeRegistry)
				.await
				.map_err(|error| error.reason)?;
			if let Some(stored) = stored {
				*guard = Some(Registry::decode_stored(&stored)?);
			}
		}
		Ok(guard.as_mut())
	}
}

impl PushWakes {
	fn relay(&self) -> Option<Arc<dyn WakeRelay>> {
		self.relay.lock().expect("wake relay mutex poisoned").clone()
	}
}

/// Give `registration` to `relay`, and wait at most [`RELAY_TIMEOUT`].
async fn register(
	relay: Arc<dyn WakeRelay>,
	registration: WakeRelayRegistration,
) -> Result<(), String> {
	futures::select! {
		answer = relay.register_wake_topics(registration).fuse() => {
			answer.map_err(|error| error.reason)
		},
		() = futures_timer::Delay::new(RELAY_TIMEOUT).fuse() => {
			Err("the push relay did not answer".to_string())
		},
	}
}

#[cfg(test)]
mod tests;
