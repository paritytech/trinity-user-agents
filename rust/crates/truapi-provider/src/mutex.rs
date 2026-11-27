//! Taking a [`Mutex`] that a panicking holder may have poisoned.

use std::sync::{Mutex, MutexGuard};

/// Lock `mutex`, blocking until it is free.
///
/// # Panics
///
/// If a previous holder panicked while holding it, so no caller reads state
/// that panic may have left half-updated. The panic is reported at the caller.
#[track_caller]
pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
	mutex.lock().expect("a previous holder of this mutex panicked")
}

/// Lock `mutex`, or `None` if a previous holder panicked while holding it.
///
/// For cleanup that a `Drop` runs. A panic there during an unwind aborts the
/// process, so cleanup skips state a panic may have left half-updated rather
/// than panicking on it the way [`lock`] does.
pub fn lock_unless_poisoned<T>(mutex: &Mutex<T>) -> Option<MutexGuard<'_, T>> {
	mutex.lock().ok()
}

// Poisoning needs a panic to unwind, which wasm32 never does.
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
