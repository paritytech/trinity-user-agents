use super::*;

fn locked(lock: Value) -> Value {
	Value::unnamed_variant("Locked", [lock])
}

fn failed_dispatch(retries: u8) -> Value {
	Value::named_variant("FailedDispatch", [("retries", Value::u128(u128::from(retries)))])
}

#[test]
fn an_unloaded_alias_decodes_as_unloaded() {
	assert_eq!(alias_state(&Value::unnamed_variant("Unloaded", [])), Some(AliasState::Unloaded));
}

#[test]
fn a_locked_alias_decodes_with_the_end_of_its_lock() {
	let lock = Value::named_composite([("reason", failed_dispatch(3)), ("until", Value::u128(1_700_000_000))]);
	assert_eq!(alias_state(&locked(lock)), Some(AliasState::Locked(1_700_000_000)));
}

#[test]
fn a_lock_without_its_end_is_unknown_rather_than_unlocked() {
	let without_end = Value::named_composite([("reason", failed_dispatch(3))]);
	assert_eq!(alias_state(&locked(without_end)), None);
	let beyond_u64 = Value::named_composite([("reason", failed_dispatch(3)), ("until", Value::u128(u128::from(u64::MAX) + 1))]);
	assert_eq!(alias_state(&locked(beyond_u64)), None);
}
