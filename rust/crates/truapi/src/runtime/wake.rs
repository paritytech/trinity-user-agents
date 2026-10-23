//! Wakes: bounded holds of the worker of a product.
//!
//! A host wakes a product when the product must do work and no screen holds
//! its worker. A wake holds one worker reference for [`CHAT_WAKE_HOLD_MS`].
//! A product has at most one wake hold, and a new wake moves its end. A
//! product extends the hold only through `Worker::begin_operation`. A product
//! gets at most [`CHAT_NOTIFICATION_WAKES_PER_HOUR`] notification wakes in
//! one rolling hour.

use core::time::Duration;

use futures::future::BoxFuture;
use std::{
	collections::{HashMap, VecDeque},
	sync::{Arc, Mutex},
};

use truapi::latest;

use crate::{
	platform::{ChatFieldError, ProductId, normalize_chat_identifier, validate_chat_room_event},
	runtime::RuntimeServices,
	unix_time::current_unix_millis,
};

/// The time one wake holds the worker, in milliseconds.
pub const CHAT_WAKE_HOLD_MS: u64 = 30_000;

/// Largest number of notification wakes of one product in one rolling hour.
pub const CHAT_NOTIFICATION_WAKES_PER_HOUR: usize = 12;

const HOUR_MS: u64 = 60 * 60 * 1000;

/// Why a host wakes a product.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum ChatWakeReason {
	/// The product started and has no room yet.
	ProductStart,
	/// The host sent a user message or a room event to the product while no
	/// screen holds the worker.
	MessageInFlight {
		/// The room of the message or the event.
		room_id: String,
	},
	/// A chat notification for this room arrived.
	Notification {
		/// The room.
		room_id: String,
	},
}

/// What a wake did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum ChatWakeOutcome {
	/// The host holds the worker until this time, in milliseconds since the
	/// Unix epoch.
	Held {
		/// The end of the hold.
		until_ms: u64,
	},
	/// The product used all its notification wakes of this hour. The core
	/// holds nothing.
	BudgetSpent,
}

/// The wake holds of each product of one host runtime.
#[derive(Default)]
pub struct ChatWakes {
	products: Mutex<HashMap<String, WakeState>>,
}

/// The wake hold and the recent notification wakes of one product.
#[derive(Default)]
struct WakeState {
	/// The core holds one worker reference for the wake.
	held: bool,
	/// Counts the holds, so that the timer of a hold that a later wake moved
	/// releases nothing.
	generation: u64,
	/// When each notification wake of the last hour came, oldest first.
	notifications: VecDeque<u64>,
}

/// What [`ChatWakes::begin`] decided.
#[derive(Debug, PartialEq, Eq)]
struct WakeStart {
	/// The end of the hold.
	until_ms: u64,
	/// The hold to release at `until_ms`.
	generation: u64,
	/// The wake took a new worker reference.
	acquired: bool,
}

impl ChatWakes {
	/// Start or move the wake hold of `product_id` at `now_ms`. `None` when the
	/// product used its notification wakes of the hour.
	fn begin(&self, product_id: &str, notification: bool, now_ms: u64) -> Option<WakeStart> {
		let mut products = self.products.lock().expect("chat wake mutex poisoned");
		let state = products.entry(product_id.to_string()).or_default();
		if notification {
			while state
				.notifications
				.front()
				.is_some_and(|woken_at| now_ms.saturating_sub(*woken_at) >= HOUR_MS)
			{
				state.notifications.pop_front();
			}
			if state.notifications.len() >= CHAT_NOTIFICATION_WAKES_PER_HOUR {
				return None;
			}
			state.notifications.push_back(now_ms);
		}
		let acquired = !state.held;
		state.held = true;
		state.generation += 1;
		Some(WakeStart {
			until_ms: now_ms + CHAT_WAKE_HOLD_MS,
			generation: state.generation,
			acquired,
		})
	}

	/// End the hold `generation` of `product_id`. `true` when it was the
	/// current hold, so that its worker reference goes.
	fn end(&self, product_id: &str, generation: u64) -> bool {
		let mut products = self.products.lock().expect("chat wake mutex poisoned");
		match products.get_mut(product_id) {
			Some(state) if state.held && state.generation == generation => {
				state.held = false;
				true
			},
			_ => false,
		}
	}
}

impl RuntimeServices {
	/// Wake `product_id` for `reason`: hold its worker for
	/// [`CHAT_WAKE_HOLD_MS`].
	pub fn wake_product(
		self: &Arc<Self>,
		product_id: &ProductId,
		reason: ChatWakeReason,
	) -> Result<ChatWakeOutcome, ChatFieldError> {
		self.wake_product_at(product_id, reason, current_unix_millis(), |hold| {
			Box::pin(futures_timer::Delay::new(hold))
		})
	}

	/// [`Self::wake_product`] at `now_ms`, with `timer` making the wait of the
	/// hold, so a test holds the clock still.
	fn wake_product_at(
		self: &Arc<Self>,
		product_id: &ProductId,
		reason: ChatWakeReason,
		now_ms: u64,
		timer: impl FnOnce(Duration) -> BoxFuture<'static, ()>,
	) -> Result<ChatWakeOutcome, ChatFieldError> {
		let reason = match reason {
			ChatWakeReason::ProductStart => ChatWakeReason::ProductStart,
			ChatWakeReason::MessageInFlight { room_id } => ChatWakeReason::MessageInFlight {
				room_id: normalize_chat_identifier("roomId", &room_id)?,
			},
			ChatWakeReason::Notification { room_id } => ChatWakeReason::Notification {
				room_id: normalize_chat_identifier("roomId", &room_id)?,
			},
		};
		let notification = matches!(reason, ChatWakeReason::Notification { .. });
		let Some(start) = self.chat_wakes.begin(product_id.as_str(), notification, now_ms) else {
			return Ok(ChatWakeOutcome::BudgetSpent);
		};
		if start.acquired {
			self.worker_ledger.acquire(product_id);
		}
		let services = self.clone();
		let product = product_id.clone();
		let hold = timer(Duration::from_millis(CHAT_WAKE_HOLD_MS));
		(self.spawner)(Box::pin(async move {
			hold.await;
			if services.chat_wakes.end(product.as_str(), start.generation) {
				services.worker_ledger.release(&product);
			}
		}));
		Ok(ChatWakeOutcome::Held { until_ms: start.until_ms })
	}

	/// Send one event about a room to `product_id`, or keep it until the
	/// product subscribes to `room_events_subscribe`.
	pub fn publish_chat_room_event(
		&self,
		product_id: &ProductId,
		item: latest::HostChatRoomEventsSubscribeItem,
	) -> Result<(), ChatFieldError> {
		let item = latest::HostChatRoomEventsSubscribeItem {
			room_id: normalize_chat_identifier("roomId", &item.room_id)?,
			event: validate_chat_room_event(item.event)?,
		};
		self.chat_room_events.publish(product_id.as_str(), item);
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use futures::{FutureExt, channel::oneshot};

	use super::*;
	use crate::test_support::{runtime_config, stub_platform, test_spawner};

	const PRODUCT: &str = "chat.dot";

	fn product() -> ProductId {
		ProductId::for_test(PRODUCT)
	}

	/// A clock that stands still: each hold waits until the test ends it.
	#[derive(Default)]
	struct PausedClock {
		holds: Mutex<Vec<Option<oneshot::Sender<()>>>>,
	}

	impl PausedClock {
		/// The timer of the next hold.
		fn timer(&self) -> impl FnOnce(Duration) -> BoxFuture<'static, ()> + '_ {
			move |hold| {
				assert_eq!(hold, Duration::from_millis(CHAT_WAKE_HOLD_MS));
				let (end, ended) = oneshot::channel();
				self.holds.lock().unwrap().push(Some(end));
				ended.map(drop).boxed()
			}
		}

		/// Let the 30 s of hold `index` pass.
		fn end(&self, index: usize) {
			let end = self.holds.lock().unwrap()[index].take().expect("a hold ends once");
			let _ = end.send(());
		}

		fn holds(&self) -> usize {
			self.holds.lock().unwrap().len()
		}
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

	/// Wait until the worker of the product has `count` references.
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

	fn notification() -> ChatWakeReason {
		ChatWakeReason::Notification { room_id: "main".to_string() }
	}

	// S71: a hold keeps the worker for 30 s, and no longer, while the clock
	// stands still until then.
	#[test]
	fn a_hold_ends_when_its_thirty_seconds_pass() {
		let services = services();
		let clock = PausedClock::default();

		let outcome = services.wake_product_at(
			&product(),
			ChatWakeReason::ProductStart,
			1_000,
			clock.timer(),
		);

		assert_eq!(outcome, Ok(ChatWakeOutcome::Held { until_ms: 31_000 }));
		references(&services, 1);
		std::thread::sleep(Duration::from_millis(50));
		assert_eq!(services.worker_ledger.count(&product()), 1, "the clock has not moved");
		clock.end(0);
		references(&services, 0);
	}

	// S71: a product that began an operation keeps its worker after the hold.
	#[test]
	fn an_operation_outlives_the_hold() {
		let services = services();
		let clock = PausedClock::default();
		services
			.wake_product_at(&product(), ChatWakeReason::ProductStart, 0, clock.timer())
			.unwrap();
		services.worker_ledger.acquire(&product());

		clock.end(0);
		references(&services, 1);
		services.worker_ledger.release(&product());
		references(&services, 0);
	}

	// D28: a second wake moves the end of the one hold; the first timer
	// releases nothing.
	#[test]
	fn a_second_wake_moves_the_end_of_the_hold() {
		let services = services();
		let clock = PausedClock::default();

		let first =
			services.wake_product_at(&product(), ChatWakeReason::ProductStart, 0, clock.timer());
		let second = services.wake_product_at(
			&product(),
			ChatWakeReason::MessageInFlight { room_id: "main".to_string() },
			10_000,
			clock.timer(),
		);

		assert_eq!(first, Ok(ChatWakeOutcome::Held { until_ms: 30_000 }));
		assert_eq!(second, Ok(ChatWakeOutcome::Held { until_ms: 40_000 }));
		references(&services, 1);
		clock.end(0);
		std::thread::sleep(Duration::from_millis(50));
		assert_eq!(services.worker_ledger.count(&product()), 1, "the moved hold still runs");
		clock.end(1);
		references(&services, 0);
	}

	// S71: the thirteenth notification wake of an hour holds nothing; an hour
	// after the first, one more is free.
	#[test]
	fn the_budget_of_notification_wakes_holds_for_an_hour() {
		let services = services();
		let clock = PausedClock::default();
		for minute in 0..CHAT_NOTIFICATION_WAKES_PER_HOUR as u64 {
			let outcome = services.wake_product_at(
				&product(),
				notification(),
				minute * 60_000,
				clock.timer(),
			);
			assert!(matches!(outcome, Ok(ChatWakeOutcome::Held { .. })));
		}
		let held = clock.holds();

		assert_eq!(
			services.wake_product_at(&product(), notification(), 59 * 60_000, clock.timer()),
			Ok(ChatWakeOutcome::BudgetSpent)
		);
		assert_eq!(clock.holds(), held, "a spent budget starts no hold");
		assert_eq!(
			services.wake_product_at(&product(), notification(), HOUR_MS, clock.timer()),
			Ok(ChatWakeOutcome::Held { until_ms: HOUR_MS + CHAT_WAKE_HOLD_MS })
		);
	}

	#[test]
	fn a_wake_holds_the_worker_once_and_a_second_wake_moves_the_end() {
		let wakes = ChatWakes::default();

		let first = wakes.begin(PRODUCT, false, 1_000).unwrap();
		let second = wakes.begin(PRODUCT, false, 5_000).unwrap();

		assert_eq!(first, WakeStart { until_ms: 31_000, generation: 1, acquired: true });
		assert_eq!(second, WakeStart { until_ms: 35_000, generation: 2, acquired: false });
		// The timer of the first hold releases nothing; the second one releases
		// the one reference, once.
		assert!(!wakes.end(PRODUCT, first.generation));
		assert!(wakes.end(PRODUCT, second.generation));
		assert!(!wakes.end(PRODUCT, second.generation));
	}

	#[test]
	fn the_thirteenth_notification_wake_of_an_hour_holds_nothing() {
		let wakes = ChatWakes::default();
		for minute in 0..CHAT_NOTIFICATION_WAKES_PER_HOUR as u64 {
			assert!(wakes.begin(PRODUCT, true, minute * 60_000).is_some());
		}

		assert_eq!(wakes.begin(PRODUCT, true, 59 * 60_000), None);
		// Other wakes do not count against the budget.
		assert!(wakes.begin(PRODUCT, false, 59 * 60_000).is_some());
		// An hour after the first notification wake, one wake is free again.
		assert!(wakes.begin(PRODUCT, true, HOUR_MS).is_some());
		assert_eq!(wakes.begin(PRODUCT, true, HOUR_MS), None);
	}

	#[test]
	fn the_budget_of_one_product_does_not_touch_another() {
		let wakes = ChatWakes::default();
		for _ in 0..CHAT_NOTIFICATION_WAKES_PER_HOUR {
			wakes.begin(PRODUCT, true, 0);
		}

		assert_eq!(wakes.begin(PRODUCT, true, 0), None);
		assert!(wakes.begin("other.dot", true, 0).is_some());
	}
}
