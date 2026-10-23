//! Call holds: a room with a call holds the worker of its product.
//!
//! A product rings or carries a call only while its worker runs, so a room
//! whose call is `Ringing` or `Active` holds one worker reference until the
//! product clears the call or deletes the room. A call rings until the caller
//! ends it or the user answers, as the native chat rings.

use std::{
	collections::HashSet,
	sync::{Arc, Mutex, PoisonError},
};

use truapi::latest;

use crate::{platform::ProductId, runtime::RuntimeServices};

/// The rooms whose call holds a worker reference, by product and room.
#[derive(Default)]
pub struct ChatCallHolds {
	rooms: Mutex<HashSet<(ProductId, String)>>,
}

impl ChatCallHolds {
	fn lock(&self) -> std::sync::MutexGuard<'_, HashSet<(ProductId, String)>> {
		self.rooms.lock().unwrap_or_else(PoisonError::into_inner)
	}
}

impl RuntimeServices {
	/// Follow the call of `room_id` that the core accepted: a call holds the
	/// worker of `product_id`, and `None` releases it.
	pub fn hold_chat_call(
		self: &Arc<Self>,
		product_id: &ProductId,
		room_id: &str,
		call: Option<&latest::ChatRoomCall>,
	) {
		let key = (product_id.clone(), room_id.to_string());
		if call.is_some() {
			if self.chat_call_holds.lock().insert(key) {
				self.worker_ledger.acquire(product_id);
			}
		} else if self.chat_call_holds.lock().remove(&key) {
			self.worker_ledger.release(product_id);
		}
	}
}

#[cfg(test)]
mod tests {
	use core::time::Duration;

	use super::*;
	use crate::test_support::{runtime_config, stub_platform, test_spawner};

	const PRODUCT: &str = "chat.dot";

	fn product() -> ProductId {
		ProductId::for_test(PRODUCT)
	}

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
		while services.worker_ledger.count(&product()) != count {
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
		latest::ChatRoomCall::Active { call_id: "c1".to_string(), video: false, started_at: 1 }
	}

	// D31: a call holds the worker once, whatever its state, until cleared.
	#[test]
	fn a_call_holds_the_worker_until_it_is_cleared() {
		let services = services();

		services.hold_chat_call(&product(), "main", Some(&ringing()));
		services.hold_chat_call(&product(), "main", Some(&active()));
		services.hold_chat_call(&product(), "other", Some(&active()));
		references(&services, 2);
		services.hold_chat_call(&product(), "main", None);
		services.hold_chat_call(&product(), "main", None);
		references(&services, 1);
		services.hold_chat_call(&product(), "other", None);
		references(&services, 0);
	}

	// A call rings until the caller ends it, as the native chat rings: no time limit drops the
	// hold.
	#[test]
	fn a_ringing_call_holds_the_worker_until_it_is_cleared() {
		let services = services();
		services.hold_chat_call(&product(), "main", Some(&ringing()));
		std::thread::sleep(Duration::from_millis(50));
		assert_eq!(services.worker_ledger.count(&product()), 1);
		services.hold_chat_call(&product(), "main", None);
		references(&services, 0);
	}
}
