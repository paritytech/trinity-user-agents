use super::*;
use crate::test_support::sso_session_info;

const PAIRED_AT: u64 = 1_800_000_000;

fn sso() -> SsoSessionInfo {
	sso_session_info().sso.expect("fixture carries an SSO session")
}

fn other_sso() -> SsoSessionInfo {
	let mut other = sso();
	other.ss_public_key = [0xEE; 32];
	other
}

#[test]
fn a_session_ends_thirty_days_after_pairing_however_recently_its_keys_changed() {
	let mut times = SessionTimes::paired(&sso(), PAIRED_AT);
	times.last_rotated_at = Some(PAIRED_AT + SESSION_LIFETIME_SECS - 1);

	assert_eq!(
		[
			times.is_expired(PAIRED_AT + SESSION_LIFETIME_SECS - 1),
			times.is_expired(PAIRED_AT + SESSION_LIFETIME_SECS),
		],
		[false, true]
	);
}

#[test]
fn a_clock_set_back_before_pairing_does_not_end_the_session() {
	let times = SessionTimes::paired(&sso(), PAIRED_AT);

	assert!(!times.is_expired(0), "a clock behind pairing time must not read as an old session");
}

#[test]
fn keys_are_due_to_change_once_more_than_a_day_old() {
	let times = SessionTimes::paired(&sso(), PAIRED_AT);

	assert_eq!(
		[
			times.is_rotation_due(PAIRED_AT),
			times.is_rotation_due(PAIRED_AT + KEY_ROTATION_INTERVAL_SECS),
			times.is_rotation_due(PAIRED_AT + KEY_ROTATION_INTERVAL_SECS + 1),
		],
		[false, false, true]
	);
}

#[test]
fn a_session_first_seen_without_times_is_due_for_a_key_change_and_starts_its_lifetime() {
	let times = SessionTimes::first_seen(&sso(), PAIRED_AT);

	assert_eq!(
		[
			times.is_rotation_due(PAIRED_AT),
			times.is_expired(PAIRED_AT + SESSION_LIFETIME_SECS - 1),
			times.is_expired(PAIRED_AT + SESSION_LIFETIME_SECS),
		],
		[true, false, true]
	);
}

#[test]
fn stored_times_belong_to_a_session_only_when_they_name_its_key() {
	let stored = SessionTimes::paired(&sso(), PAIRED_AT).to_stored();

	assert_eq!(
		[
			SessionTimes::from_stored(Some(&stored), &sso()),
			SessionTimes::from_stored(Some(&stored), &other_sso()),
			SessionTimes::from_stored(None, &sso()),
		],
		[
			StoredSessionTimes::Current(SessionTimes::paired(&sso(), PAIRED_AT)),
			StoredSessionTimes::Replaceable,
			StoredSessionTimes::Replaceable,
		]
	);
}

#[test]
fn a_value_this_core_cannot_decode_is_unreadable_not_replaceable() {
	let mut trailing = SessionTimes::paired(&sso(), PAIRED_AT).to_stored();
	trailing.push(0);
	let mut newer_version = SessionTimes::paired(&sso(), PAIRED_AT).to_stored();
	newer_version[0] = 1;

	assert_eq!(
		[
			SessionTimes::from_stored(Some(&trailing), &sso()),
			SessionTimes::from_stored(Some(&newer_version), &sso()),
		],
		[StoredSessionTimes::Unreadable, StoredSessionTimes::Unreadable],
		"a newer core's record must be left for it, not replaced by this core's first sight"
	);
}
