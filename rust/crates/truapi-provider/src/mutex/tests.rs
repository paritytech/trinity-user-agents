use std::{sync::Mutex, thread};

use super::lock;

/// A holder that panics mid-update can leave the data half-written, so the
/// next caller must not be handed it.
#[test]
#[should_panic(expected = "a previous holder of this mutex panicked")]
fn a_poisoned_mutex_is_not_handed_out() {
	let mutex = Mutex::new(0);
	thread::scope(|scope| {
		let holder = scope.spawn(|| {
			let _guard = lock(&mutex);
			panic!("the holder panics while holding the lock");
		});
		assert!(holder.join().is_err(), "only a holder that panicked poisons the mutex");
	});
	let _guard = lock(&mutex);
}
