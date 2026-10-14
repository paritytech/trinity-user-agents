//! Core-owned auth/session UI state machine. Every [`AuthState`] emission to
//! the host funnels through [`AuthStateMachine`], so transitions stay ordered
//! and a stale session-store tick can never tear down an in-flight pairing.

use std::sync::{Arc, Mutex};

use crate::platform::{AuthPresenter, AuthState, LoginFailureKind, Platform, SessionUiInfo};
use futures::channel::oneshot;

use crate::runtime::login_failure::classify_login_failure;

/// Serialized auth-state machine bound to the platform's `auth_state_changed`
/// sink. Each transition mutates under the lock, releases it, then emits the
/// new state (when it actually changed), so `auth_state_changed` handlers may
/// safely re-enter the runtime (e.g. a host cancelling the login it just
/// observed). The cancel channel for an in-flight login lives inside the
/// in-flight login states, making its registration atomic with the transition.
///
/// The boot-time restore and explicit session activations call
/// [`AuthStateMachine::announce_current`] when done, so the host gets an
/// opening state even when nothing changed. Only the first emission can come
/// from an announcement; everything after it is a real change.
#[derive(Clone)]
pub struct AuthStateMachine {
	platform: Arc<dyn Platform>,
	inner: Arc<Mutex<AuthStateInner>>,
}

#[derive(Default)]
struct AuthStateInner {
	state: AuthState,
	/// Increments on every `pairing_started`; lets an abandoned flow's reset
	/// guard distinguish its own login from a newer flow's.
	pairing_epoch: u64,
	/// Resolves the in-flight login's cancel receiver. Present while the state
	/// is `Pairing` or `Authenticating`.
	cancel_tx: Option<oneshot::Sender<()>>,
	/// Whether the host has observed any state yet. Gates the opening
	/// announcement so it can only ever produce the first emission.
	announced: bool,
}

impl AuthStateMachine {
	/// Create an auth state machine that reports transitions to `platform`.
	pub fn new(platform: Arc<dyn Platform>) -> Self {
		Self { platform, inner: Arc::new(Mutex::new(AuthStateInner::default())) }
	}

	/// Enter `Pairing`. Returns the cancel receiver and the pairing epoch, or
	/// `None` when a pairing is already in flight (single-flight guard).
	pub fn pairing_started(&self, deeplink: String) -> Option<(oneshot::Receiver<()>, u64)> {
		let (cancel_tx, cancel_rx) = oneshot::channel();
		let epoch = self.transition(|inner| {
			if matches!(inner.state, AuthState::Pairing { .. } | AuthState::Authenticating) {
				return None;
			}
			inner.state = AuthState::Pairing { deeplink };
			inner.pairing_epoch = inner.pairing_epoch.wrapping_add(1);
			inner.cancel_tx = Some(cancel_tx);
			Some(inner.pairing_epoch)
		})?;
		Some((cancel_rx, epoch))
	}

	/// `Pairing` -> `Authenticating`: the wallet accepted the pairing request
	/// and the core is resolving and persisting the session.
	pub fn authentication_started(&self, epoch: u64) {
		self.transition(|inner| {
			if !matches!(inner.state, AuthState::Pairing { .. }) || inner.pairing_epoch != epoch {
				return None;
			}
			inner.state = AuthState::Authenticating;
			Some(())
		});
	}

	/// Active login -> `LoginFailed`: the in-flight login reported a failure.
	/// The kind is recovered from `reason`, which is the only form the wallet
	/// reports a refusal in.
	pub fn login_failed(&self, reason: String) {
		self.transition(|inner| {
			if !matches!(inner.state, AuthState::Pairing { .. } | AuthState::Authenticating) {
				return None;
			}
			inner.cancel_tx = None;
			inner.state = AuthState::LoginFailed { kind: classify_login_failure(&reason), reason };
			Some(())
		});
	}

	/// `Disconnected`/`LoginFailed` -> `LoginFailed`: a login failed before
	/// it reached `Pairing` (device identity or bootstrap errors). A no-op
	/// while another login is active, so a concurrent second login attempt
	/// failing early cannot tear down the first one's presentation.
	pub fn login_failed_before_pairing(&self, reason: String) {
		self.transition(|inner| {
			if matches!(
				inner.state,
				AuthState::Pairing { .. } | AuthState::Authenticating | AuthState::Connected(_)
			) {
				return None;
			}
			// Pre-pairing failures are the pairing host's own (device identity,
			// bootstrap); allowance exhaustion is only ever wallet-reported.
			inner.state = AuthState::LoginFailed { kind: LoginFailureKind::Other, reason };
			Some(())
		});
	}

	/// Active login/`LoginFailed` -> `Disconnected` (host cancelled or
	/// dismissed). Wakes the in-flight login, which resolves as `Rejected`.
	pub fn login_cancelled(&self) {
		self.transition(|inner| {
			if !matches!(
				inner.state,
				AuthState::Pairing { .. } |
					AuthState::Authenticating |
					AuthState::LoginFailed { .. }
			) {
				return None;
			}
			if let Some(cancel_tx) = inner.cancel_tx.take() {
				let _ = cancel_tx.send(());
			}
			inner.state = AuthState::Disconnected;
			Some(())
		});
	}

	/// Any state -> `Connected`. A login in flight is cancelled: another
	/// runtime won the race, and the waking flow resolves as
	/// `AlreadyConnected`. Emits only when the connected info changed.
	pub fn connected(&self, info: &SessionUiInfo) {
		self.transition(|inner| {
			if let Some(cancel_tx) = inner.cancel_tx.take() {
				let _ = cancel_tx.send(());
			}
			if matches!(&inner.state, AuthState::Connected(current) if current == info) {
				return None;
			}
			inner.state = AuthState::Connected(info.clone());
			Some(())
		});
	}

	/// Session store reports no session. A no-op while a login is active: the
	/// flow owns its own terminal transition, and a boot-time store tick must
	/// not tear down the login UI.
	pub fn store_disconnected(&self) {
		self.transition(|inner| {
			if matches!(
				inner.state,
				AuthState::Pairing { .. } | AuthState::Authenticating | AuthState::Disconnected
			) {
				return None;
			}
			inner.state = AuthState::Disconnected;
			Some(())
		});
	}

	/// Reset a login left behind by a dropped future, but only when it still
	/// belongs to `epoch` (a newer flow is left alone).
	pub fn reset_abandoned_pairing(&self, epoch: u64) {
		self.transition(|inner| {
			if !matches!(inner.state, AuthState::Pairing { .. } | AuthState::Authenticating) ||
				inner.pairing_epoch != epoch
			{
				return None;
			}
			inner.cancel_tx = None;
			inner.state = AuthState::Disconnected;
			Some(())
		});
	}

	/// Report the current state to a host that has not observed one yet, so a
	/// session activation that changed nothing — because it found no session,
	/// or because it failed before any transition could run — still answers the
	/// host instead of leaving it in silence. A no-op once any state has been
	/// emitted: it announces, it never repeats.
	pub fn announce_current(&self) {
		let mut inner = self.inner.lock().expect("auth state mutex poisoned");
		if inner.announced {
			return;
		}
		inner.announced = true;
		let state = inner.state.clone();
		drop(inner);
		AuthPresenter::auth_state_changed(self.platform.as_ref(), state);
	}

	/// Run `apply` under the lock; when it changed the state (returned
	/// `Some`), emit the new state to the host after releasing the lock.
	fn transition<T>(&self, apply: impl FnOnce(&mut AuthStateInner) -> Option<T>) -> Option<T> {
		let mut inner = self.inner.lock().expect("auth state mutex poisoned");
		let applied = apply(&mut inner)?;
		inner.announced = true;
		let state = inner.state.clone();
		drop(inner);
		AuthPresenter::auth_state_changed(self.platform.as_ref(), state);
		Some(applied)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::test_support::stub_platform;

	#[test]
	fn announcing_a_signed_out_boot_emits_disconnected_once() {
		let platform = stub_platform();
		let machine = AuthStateMachine::new(platform.clone());

		machine.store_disconnected();
		machine.announce_current();

		assert_eq!(
			*platform.auth_states.lock().expect("auth state list mutex poisoned"),
			vec![AuthState::Disconnected],
			"a host that boots signed out must be told so, not left in silence"
		);

		machine.announce_current();

		assert_eq!(
			*platform.auth_states.lock().expect("auth state list mutex poisoned"),
			vec![AuthState::Disconnected],
			"a later announcement adds nothing to a host that already has an answer"
		);
	}

	#[test]
	fn a_wallet_reported_exhausted_period_reaches_the_host_as_a_typed_kind() {
		let platform = stub_platform();
		let machine = AuthStateMachine::new(platform.clone());
		let (_cancel_rx, epoch) = machine
			.pairing_started("polkadotapp://pair".to_string())
			.expect("login should start");
		machine.authentication_started(epoch);

		machine.login_failed("no free StatementStore slot in period 7 (max 8)".to_string());

		assert_eq!(
			platform.auth_states.lock().expect("auth state list mutex poisoned").last(),
			Some(&AuthState::LoginFailed {
				kind: LoginFailureKind::NoFreeAllowanceSlots,
				reason: "no free StatementStore slot in period 7 (max 8)".to_string(),
			}),
			"a host must be able to branch on the kind without reading the reason"
		);
	}

	#[test]
	fn announcing_after_a_restored_session_leaves_connected_alone() {
		let platform = stub_platform();
		let machine = AuthStateMachine::new(platform.clone());
		let session = SessionUiInfo::default();

		machine.connected(&session);
		machine.announce_current();

		assert_eq!(
			*platform.auth_states.lock().expect("auth state list mutex poisoned"),
			vec![AuthState::Connected(session)],
			"the opening emission is the activation's outcome, not a placeholder"
		);
	}

	#[test]
	fn a_transition_before_the_first_activation_does_not_spend_the_announcement() {
		let platform = stub_platform();
		let machine = AuthStateMachine::new(platform.clone());

		// A host may cancel a login or take a session-store tick before it
		// activates. Neither changed the state, so neither may emit: a
		// spurious `Disconnected` here flashes signed out at a signed-in user.
		machine.login_cancelled();
		machine.store_disconnected();

		assert!(
			platform.auth_states.lock().expect("auth state list mutex poisoned").is_empty(),
			"only an activation announces; unrelated no-op transitions stay silent"
		);

		let session = SessionUiInfo::default();
		machine.connected(&session);
		machine.announce_current();

		assert_eq!(
			*platform.auth_states.lock().expect("auth state list mutex poisoned"),
			vec![AuthState::Connected(session)],
			"the restored session is the host's first and only opening state"
		);
	}

	#[test]
	fn a_pre_pairing_failure_is_never_reported_as_an_exhausted_period() {
		let platform = stub_platform();
		let machine = AuthStateMachine::new(platform.clone());

		// Allowance exhaustion is only ever wallet-reported, so this path does
		// not classify even when the text would otherwise match.
		machine.login_failed_before_pairing(
			"no free StatementStore slot in period 7 (max 8)".to_string(),
		);

		assert_eq!(
			platform.auth_states.lock().expect("auth state list mutex poisoned").last(),
			Some(&AuthState::LoginFailed {
				kind: LoginFailureKind::Other,
				reason: "no free StatementStore slot in period 7 (max 8)".to_string(),
			})
		);
	}

	#[test]
	fn pairing_started_refuses_a_second_login_while_authenticating() {
		let platform = stub_platform();
		let machine = AuthStateMachine::new(platform.clone());
		let (_cancel_rx, epoch) = machine
			.pairing_started("polkadotapp://first".to_string())
			.expect("first login should start");
		machine.authentication_started(epoch);

		assert!(
			machine.pairing_started("polkadotapp://second".to_string()).is_none(),
			"an authenticating login must retain the single-flight guard"
		);
		assert_eq!(
			*platform.auth_states.lock().expect("auth state list mutex poisoned"),
			vec![
				AuthState::Pairing { deeplink: "polkadotapp://first".to_string() },
				AuthState::Authenticating,
			]
		);
	}

	#[test]
	fn login_cancelled_while_authenticating_disconnects_and_wakes_the_login() {
		let platform = stub_platform();
		let machine = AuthStateMachine::new(platform.clone());
		let (cancel_rx, epoch) = machine
			.pairing_started("polkadotapp://pair".to_string())
			.expect("login should start");
		machine.authentication_started(epoch);

		machine.login_cancelled();

		futures::executor::block_on(cancel_rx).expect("cancel signal should be delivered");
		assert_eq!(
			*platform.auth_states.lock().expect("auth state list mutex poisoned"),
			vec![
				AuthState::Pairing { deeplink: "polkadotapp://pair".to_string() },
				AuthState::Authenticating,
				AuthState::Disconnected,
			]
		);
	}
}
