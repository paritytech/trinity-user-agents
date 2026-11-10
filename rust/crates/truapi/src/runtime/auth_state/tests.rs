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
		vec![AuthState::Connected(Box::new(session))],
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
		vec![AuthState::Connected(Box::new(session))],
		"the restored session is the host's first and only opening state"
	);
}

#[test]
fn a_pre_pairing_failure_is_never_reported_as_an_exhausted_period() {
	let platform = stub_platform();
	let machine = AuthStateMachine::new(platform.clone());

	// Allowance exhaustion is only ever wallet-reported, so this path does
	// not classify even when the text would otherwise match.
	machine
		.login_failed_before_pairing("no free StatementStore slot in period 7 (max 8)".to_string());

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

#[test]
fn a_disconnect_during_a_connected_callback_is_delivered_after_that_callback() {
	let platform = stub_platform();
	let machine = AuthStateMachine::new(platform.clone());
	let observed = Arc::new(parking_lot::Mutex::new(Vec::new()));
	let delivered = observed.clone();
	let (started, ready) = std::sync::mpsc::channel();
	let (release, released) = std::sync::mpsc::channel();
	let released = parking_lot::Mutex::new(released);
	*platform.on_auth_state.lock().unwrap() = Some(Arc::new(move |state| {
		if matches!(state, AuthState::Connected(_)) {
			started.send(()).unwrap();
			released.lock().recv().unwrap();
		}
		delivered.lock().push(state.clone());
	}));
	let connecting = machine.clone();
	let worker = std::thread::spawn(move || connecting.connected(&SessionUiInfo::default()));
	ready.recv().unwrap();
	machine.store_disconnected();
	release.send(()).unwrap();
	worker.join().unwrap();
	assert_eq!(
		*observed.lock(),
		vec![AuthState::Connected(Box::new(SessionUiInfo::default())), AuthState::Disconnected],
	);
}

#[test]
fn a_connected_callback_can_reenter_and_disconnect() {
	let platform = stub_platform();
	let machine = AuthStateMachine::new(platform.clone());
	let reentrant = machine.clone();
	*platform.on_auth_state.lock().unwrap() = Some(Arc::new(move |state| {
		if matches!(state, AuthState::Connected(_)) {
			reentrant.store_disconnected();
		}
	}));
	machine.connected(&SessionUiInfo::default());
	assert_eq!(
		*platform.auth_states.lock().unwrap(),
		vec![AuthState::Connected(Box::new(SessionUiInfo::default())), AuthState::Disconnected],
	);
	platform.on_auth_state.lock().unwrap().take();
}
