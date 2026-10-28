//! Shared resource controls for the test host.

use super::authority::AuthorityError;
use core::sync::atomic::{AtomicBool, Ordering};
use futures::channel::oneshot;
use std::{collections::HashSet, sync::Mutex};

/// Test-host allocation and login behavior shared by its wallet and product host.
#[derive(Default)]
pub struct TestResourceControls {
	unchecked: AtomicBool,
	withheld: Mutex<HashSet<String>>,
	/// Held login requests, or `None` while logins are not held.
	held_logins: Mutex<Option<HeldLogins>>,
}

/// Login requests waiting for a test's answer.
#[derive(Default)]
struct HeldLogins {
	/// Each held request's answer: approved or declined.
	waiting: Vec<oneshot::Sender<bool>>,
	/// An answer given while nothing was held, kept for the next request.
	next: Option<bool>,
}

impl TestResourceControls {
	/// Whether allocation is answered as granted without performing it.
	pub fn grants_allowances_unchecked(&self) -> bool {
		self.unchecked.load(Ordering::Relaxed)
	}

	/// Answer resource allocation as granted without performing it.
	pub fn set_grant_allowances_unchecked(&self, granted: bool) {
		self.unchecked.store(granted, Ordering::Relaxed);
	}

	/// Replace refused resource tags; SmartContractAllowance covers every index.
	pub fn set_withheld_resources(&self, tags: Vec<String>) {
		*self.withheld.lock().expect("withheld resource mutex poisoned") =
			tags.into_iter().collect();
	}

	/// Whether `resource` is answered as refused.
	pub fn withholds(&self, resource: &truapi::latest::AllocatableResource) -> bool {
		let tag = match resource {
			truapi::latest::AllocatableResource::StatementStoreAllowance => {
				"StatementStoreAllowance"
			},
			truapi::latest::AllocatableResource::BulletinAllowance => "BulletinAllowance",
			truapi::latest::AllocatableResource::SmartContractAllowance(_) => {
				"SmartContractAllowance"
			},
			truapi::latest::AllocatableResource::AutoSigning => "AutoSigning",
		};
		self.withheld.lock().expect("withheld resource mutex poisoned").contains(tag)
	}

	/// Hold later signed-out login requests while `hold` is set, declining
	/// those held so far.
	pub fn set_hold_logins(&self, hold: bool) {
		// Dropping the senders declines every held request.
		*self.held_logins.lock().expect("held logins mutex poisoned") =
			hold.then(HeldLogins::default);
	}

	/// Answer the held login requests, or the next one when none is held.
	///
	/// Nothing happens while logins are not held.
	pub fn answer_held_logins(&self, approved: bool) {
		let mut held = self.held_logins.lock().expect("held logins mutex poisoned");
		let Some(held) = held.as_mut() else { return };
		held.waiting.retain(|answer| !answer.is_canceled());
		if held.waiting.is_empty() {
			held.next = Some(approved);
			return;
		}
		for answer in held.waiting.drain(..) {
			let _ = answer.send(approved);
		}
	}

	/// The answer given while nothing was held, which the next login request
	/// takes whether or not a session is active.
	pub fn take_login_answer(&self) -> Option<bool> {
		self.held_logins
			.lock()
			.expect("held logins mutex poisoned")
			.as_mut()?
			.next
			.take()
	}

	/// Resolves to whether the request was approved, or `None` while logins
	/// are not held. A dropped answer is a decline.
	pub fn hold_login(&self) -> Option<oneshot::Receiver<bool>> {
		let mut held = self.held_logins.lock().expect("held logins mutex poisoned");
		let held = held.as_mut()?;
		let (answer, answered) = oneshot::channel();
		held.waiting.retain(|waiting| !waiting.is_canceled());
		held.waiting.push(answer);
		Some(answered)
	}

	/// Withholding also applies to implicit native allowance access.
	pub fn refuse_withheld(
		&self,
		resource: &truapi::latest::AllocatableResource,
	) -> Result<(), AuthorityError> {
		if self.withholds(resource) {
			return Err(AuthorityError::Rejected);
		}
		Ok(())
	}
}
