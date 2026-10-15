//! Call holds: a room with a call holds the worker of its product.
//!
//! A product rings or carries a call only while its worker runs, so a room
//! whose call is `Ringing` or `Active` holds one worker reference until the
//! product clears the call or deletes the room. A `Ringing` call holds for at
//! most [`CHAT_RINGING_HOLD_MS`], the time a host rings before it clears the
//! banner.

use core::time::Duration;
use std::{
	collections::HashMap,
	sync::{Arc, Mutex, PoisonError},
};

use futures::future::BoxFuture;
use truapi::latest;

use crate::runtime::RuntimeServices;

/// The longest time a `Ringing` call holds the worker, in milliseconds.
pub const CHAT_RINGING_HOLD_MS: u64 = 60_000;

/// The rooms whose call holds a worker reference, by product and room, each
/// with the generation of its call.
#[derive(Default)]
pub struct ChatCallHolds {
	rooms: Mutex<HashMap<(String, String), u64>>,
	generations: Mutex<u64>,
}

impl ChatCallHolds {
	fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<(String, String), u64>> {
		self.rooms.lock().unwrap_or_else(PoisonError::into_inner)
	}

	fn next_generation(&self) -> u64 {
		let mut generations = self.generations.lock().unwrap_or_else(PoisonError::into_inner);
		*generations += 1;
		*generations
	}
}

impl RuntimeServices {
	/// Follow the call of `room_id` that the core accepted: a call holds the
	/// worker of `product_id`, and `None` releases it.
	pub fn hold_chat_call(
		self: &Arc<Self>,
		product_id: &str,
		room_id: &str,
		call: Option<&latest::ChatRoomCall>,
	) {
		self.hold_chat_call_with(product_id, room_id, call, |hold| {
			Box::pin(futures_timer::Delay::new(hold))
		});
	}

	/// [`Self::hold_chat_call`] with `timer` making the wait of a `Ringing`
	/// hold, so a test holds the clock still.
	fn hold_chat_call_with(
		self: &Arc<Self>,
		product_id: &str,
		room_id: &str,
		call: Option<&latest::ChatRoomCall>,
		timer: impl FnOnce(Duration) -> BoxFuture<'static, ()>,
	) {
		let key = (product_id.to_string(), room_id.to_string());
		let Some(call) = call else {
			if self.chat_call_holds.lock().remove(&key).is_some() {
				self.worker_ledger.release(product_id);
			}
			return;
		};
		let generation = self.chat_call_holds.next_generation();
		if self.chat_call_holds.lock().insert(key.clone(), generation).is_none() {
			self.worker_ledger.acquire(product_id);
		}
		if matches!(call, latest::ChatRoomCall::Ringing { .. }) {
			let services = self.clone();
			let ring = timer(Duration::from_millis(CHAT_RINGING_HOLD_MS));
			(self.spawner)(Box::pin(async move {
				ring.await;
				let mut rooms = services.chat_call_holds.lock();
				if rooms.get(&key) == Some(&generation) {
					rooms.remove(&key);
					drop(rooms);
					services.worker_ledger.release(&key.0);
				}
			}));
		}
	}
}

#[cfg(test)]
mod tests {
	use futures::{FutureExt, channel::oneshot};

	use super::*;
	use crate::test_support::{runtime_config, stub_platform, test_spawner};

	const PRODUCT: &str = "chat.dot";

	fn services() -> Arc<RuntimeServices> {
		let (config, _) = runtime_config(PRODUCT);
		RuntimeServices::new(
			stub_platform(),
			config.host.host_info.clone(),
			config.people_chain_genesis_hash,
			config.bulletin_chain_genesis_hash,
			config.asset_hub_chain_genesis_hash,
			test_spawner(),
		)
	}

	fn references(services: &RuntimeServices, count: usize) {
		let deadline = std::time::Instant::now() + Duration::from_secs(5);
		while services.worker_ledger.count(PRODUCT) != count {
			assert!(
				std::time::Instant::now() < deadline,
				"the worker never had {count} references"
			);
			std::thread::sleep(Duration::from_millis(5));
		}
	}

	fn ringing() -> latest::ChatRoomCall {
		latest::ChatRoomCall::Ringing { call_id: "c1".to_string(), video: false, caller: None }
	}

	fn active() -> latest::ChatRoomCall {
		latest::ChatRoomCall::Active {
			call_id: "c1".to_string(),
			video: false,
			started_at: 1,
			media_session: None,
		}
	}

	/// A timer that never ends, for a hold the test ends itself.
	fn never(_: Duration) -> BoxFuture<'static, ()> {
		futures::future::pending().boxed()
	}

	// D31: a call holds the worker once, whatever its state, until cleared.
	#[test]
	fn a_call_holds_the_worker_until_it_is_cleared() {
		let services = services();

		services.hold_chat_call_with(PRODUCT, "main", Some(&ringing()), never);
		services.hold_chat_call_with(PRODUCT, "main", Some(&active()), never);
		services.hold_chat_call_with(PRODUCT, "other", Some(&active()), never);
		references(&services, 2);
		services.hold_chat_call_with(PRODUCT, "main", None, never);
		services.hold_chat_call_with(PRODUCT, "main", None, never);
		references(&services, 1);
		services.hold_chat_call_with(PRODUCT, "other", None, never);
		references(&services, 0);
	}

	// S73: a call that rings for 60 s without an answer holds no more; one
	// that became active before keeps its hold.
	#[test]
	fn a_ringing_call_holds_for_sixty_seconds_at_most() {
		let services = services();
		let (ring, rang) = oneshot::channel::<()>();
		services.hold_chat_call_with(PRODUCT, "main", Some(&ringing()), |hold| {
			assert_eq!(hold, Duration::from_millis(CHAT_RINGING_HOLD_MS));
			rang.map(drop).boxed()
		});
		references(&services, 1);
		let _ = ring.send(());
		references(&services, 0);

		let (ring, rang) = oneshot::channel::<()>();
		services.hold_chat_call_with(PRODUCT, "main", Some(&ringing()), |_| rang.map(drop).boxed());
		services.hold_chat_call_with(PRODUCT, "main", Some(&active()), never);
		let _ = ring.send(());
		std::thread::sleep(Duration::from_millis(50));
		assert_eq!(services.worker_ledger.count(PRODUCT), 1, "the active call keeps its hold");
	}
}
