//! Each product's worker within one session, and the only code that changes it.

use core::sync::atomic::Ordering;
use std::{collections::HashMap, sync::Arc};

use futures::channel::mpsc;

use super::{NativeProductExecution, NativeWorkerExecutionState, StartedAttempt};
use crate::{
	background_task::BackgroundTask, native::subscribers::Subscribers, platform::ProductId,
};

impl NativeProductExecution {
	fn has_live_transport(&self) -> bool {
		self.product_control
			.lock()
			.expect("product control mutex poisoned")
			.as_ref()
			.is_some_and(crate::ProductRuntimeControl::is_connected)
	}
}

/// One of the two things a starting worker waits for before it runs.
pub enum Signal {
	/// The engine reported that the product's module started.
	ModuleStarted,
	/// The execution's bridge authenticated a connection.
	Connected,
}

enum Phase {
	Starting {
		owner: Option<Arc<NativeProductExecution>>,
		module_ready: bool,
		/// Fails the attempt if it is still starting after [`super::STARTUP_DEADLINE`]. Dropped,
		/// and so cancelled, with this phase.
		_timeout: BackgroundTask,
	},
	Running(Arc<NativeProductExecution>),
}

enum Entry {
	Active { phase: Phase, started: Arc<StartedAttempt> },
	Retrying,
}

impl Entry {
	fn state(&self) -> NativeWorkerExecutionState {
		match self {
			Self::Active { phase: Phase::Starting { .. }, .. } | Self::Retrying => {
				NativeWorkerExecutionState::Pending
			},
			Self::Active { phase: Phase::Running(owner), .. } => {
				NativeWorkerExecutionState::Running { execution: owner.clone() }
			},
		}
	}
}

/// An entry the registry no longer holds.
///
/// Stopping it calls the worker engine, so the caller does that with the session unlocked.
pub struct Removed(Entry);

impl Removed {
	/// Shut the attempt's execution down, then stop it on its engine.
	pub fn stop(self) {
		if let Entry::Active { phase, started } = self.0 {
			match phase {
				Phase::Starting { owner: Some(owner), .. } | Phase::Running(owner) => {
					owner.shutdown()
				},
				Phase::Starting { owner: None, .. } => {},
			}
			drop(started);
		}
	}
}

/// Each product's worker and the observers following it.
///
/// Every change to an entry goes through a method here, and each one that observers can see
/// tells them. The methods that act on an attempt take only its product, so their caller is the
/// attempt the ledger counts as current, which `NativeWorkerAttempt::current` establishes.
#[derive(Default)]
pub struct Registry {
	entries: HashMap<ProductId, Entry>,
	observers: Subscribers<ProductId, NativeWorkerExecutionState>,
}

impl Registry {
	fn state(&self, product: &ProductId) -> NativeWorkerExecutionState {
		self.entries
			.get(product)
			.map(Entry::state)
			.unwrap_or(NativeWorkerExecutionState::Idle)
	}

	fn changed(&mut self, product: &ProductId) {
		self.observers.send(product, &self.state(product));
	}

	#[must_use]
	fn insert(&mut self, product: &ProductId, entry: Entry) -> Option<Removed> {
		let pending = |registry: &Self| {
			matches!(registry.state(product), NativeWorkerExecutionState::Pending)
		};
		let was_pending = pending(self);
		let previous = self.entries.insert(product.clone(), entry);
		if !(was_pending && pending(self)) {
			self.changed(product);
		}
		previous.map(Removed)
	}

	/// Follow the product: its current state first, then each change.
	///
	/// # Panics
	///
	/// If the receiving end of `observer` is already closed.
	pub fn observe(
		&mut self,
		product: ProductId,
		observer: mpsc::UnboundedSender<NativeWorkerExecutionState>,
	) {
		observer
			.unbounded_send(self.state(&product))
			.expect("worker observation is open");
		self.observers.register(product, observer);
	}

	/// Whether the product's worker runs.
	pub fn is_running(&self, product: &ProductId) -> bool {
		matches!(self.entries.get(product), Some(Entry::Active { phase: Phase::Running(_), .. }))
	}

	/// Enter a new attempt as starting, returning the entry it replaces.
	#[must_use]
	pub fn begin(
		&mut self,
		product: &ProductId,
		timeout: BackgroundTask,
		started: Arc<StartedAttempt>,
	) -> Option<Removed> {
		let phase = Phase::Starting { owner: None, module_ready: false, _timeout: timeout };
		self.insert(product, Entry::Active { phase, started })
	}

	/// Give the starting attempt the execution `open` builds.
	///
	/// `None`, without calling `open`, unless the attempt is starting and has no execution yet.
	pub fn attach(
		&mut self,
		product: &ProductId,
		open: impl FnOnce() -> Arc<NativeProductExecution>,
	) -> Option<Arc<NativeProductExecution>> {
		let Some(Entry::Active { phase: Phase::Starting { owner, .. }, .. }) =
			self.entries.get_mut(product)
		else {
			return None;
		};
		if owner.is_some() {
			return None;
		}
		Some(owner.insert(open()).clone())
	}

	/// Record a signal for the product's attempt.
	///
	/// A starting attempt runs once its module started and its connection is live. A running one
	/// that gains a live connection is announced to observers again. Returns false when the
	/// signal changes nothing: the product has no active attempt, the attempt has no execution
	/// or one that was shut down, or it runs and the signal is not a live connection.
	pub fn ready(&mut self, product: &ProductId, signal: Signal) -> bool {
		let Some(Entry::Active { phase, .. }) = self.entries.get_mut(product) else {
			return false;
		};
		match phase {
			Phase::Running(owner)
				if matches!(signal, Signal::Connected) && owner.has_live_transport() => {},
			Phase::Starting { owner: Some(owner), module_ready, .. }
				if !owner.closed.load(Ordering::Acquire) =>
			{
				*module_ready |= matches!(signal, Signal::ModuleStarted);
				if !*module_ready || !owner.has_live_transport() {
					return true;
				}
				*phase = Phase::Running(owner.clone());
			},
			_ => return false,
		}
		self.changed(product);
		true
	}

	/// Withdraw a running worker that has no live connection left.
	///
	/// It is then starting, with its module already started, until [`Self::ready`] sees a new
	/// connection. `timeout` builds the deadline for that wait and is called only on a withdrawal.
	pub fn lost(&mut self, product: &ProductId, timeout: impl FnOnce() -> BackgroundTask) {
		let Some(Entry::Active { phase, .. }) = self.entries.get_mut(product) else { return };
		let Phase::Running(owner) = phase else { return };
		if owner.has_live_transport() {
			return;
		}
		*phase =
			Phase::Starting { owner: Some(owner.clone()), module_ready: true, _timeout: timeout() };
		self.changed(product);
	}

	/// End the attempt after a failure, leaving the product waiting for its next one.
	#[must_use]
	pub fn fail(&mut self, product: &ProductId) -> Option<Removed> {
		let Some(Entry::Active { .. }) = self.entries.get(product) else { return None };
		self.insert(product, Entry::Retrying)
	}

	/// [`Self::fail`] an attempt that is still starting when its deadline passes.
	#[must_use]
	pub fn time_out(&mut self, product: &ProductId) -> Option<Removed> {
		let Some(Entry::Active { phase: Phase::Starting { .. }, .. }) = self.entries.get(product)
		else {
			return None;
		};
		self.fail(product)
	}

	/// Take the product's entry out, when its demand ends or a reload replaces its worker.
	#[must_use]
	pub fn end(&mut self, product: &ProductId) -> Option<Removed> {
		let removed = self.entries.remove(product)?;
		self.changed(product);
		Some(Removed(removed))
	}
}

impl Drop for Registry {
	fn drop(&mut self) {
		for (_, entry) in self.entries.drain() {
			Removed(entry).stop();
		}
	}
}
