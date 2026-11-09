use trinity_stack::{Descriptor, Refresh, Served, StackReader};

use super::*;
use crate::{
	stack::{StackChains, signed_pointer},
	test_support::{sign_test_descriptor, test_descriptor},
};

const PEOPLE: [u8; 32] = [0xa1; 32];
const WIPED_PEOPLE: [u8; 32] = [0xa2; 32];
const BULLETIN: [u8; 32] = [0xb2; 32];
const START: i64 = 1_800_000_000;

/// The test stack's descriptor at `sequence`, on `people`, in `generation`.
fn descriptor(sequence: u32, generation: &str, people: [u8; 32]) -> serde_json::Value {
	let mut descriptor = test_descriptor("paseo", people, BULLETIN, None);
	descriptor["sequence"] = sequence.into();
	descriptor["generation"] = generation.into();
	descriptor
}

fn signed(descriptor: &serde_json::Value) -> Vec<u8> {
	sign_test_descriptor(descriptor).1
}

/// What [`StackStatus::descriptor`] holds for `descriptor`.
fn plain(descriptor: &serde_json::Value) -> String {
	let descriptor: Descriptor =
		serde_json::from_value(descriptor.clone()).expect("test descriptor is valid");
	serde_json::to_string(&descriptor).expect("a descriptor always serialises")
}

fn load(bundled: &serde_json::Value, stored: Option<&[u8]>) -> StackSource {
	let (link, bytes) = sign_test_descriptor(bundled);
	let pointer = signed_pointer(&link).expect("test link carries a key");
	StackSource::load(pointer, &bytes, stored, false, START).expect("bundled copy is valid")
}

fn serve(source: &mut StackSource, descriptor: &serde_json::Value, now: i64) -> Option<Vec<u8>> {
	let bytes = signed(descriptor);
	source.fetched(Served::Document { bytes: &bytes, etag: None }, now)
}

fn status(
	descriptor: &serde_json::Value,
	last_refresh: Option<StackRefreshOutcome>,
) -> StackStatus {
	StackStatus {
		descriptor: plain(descriptor),
		stale: false,
		expired: false,
		restart_required: false,
		new_generation: false,
		last_refresh,
	}
}

fn people(source: &StackSource) -> [u8; 32] {
	let StackChains { people, .. } = *source.stack().chains();
	people
}

/// A stack never waits on its descriptor's server: the bundled copy is enough
/// to start, and the first fetch is only asked for.
#[test]
fn the_bundled_copy_starts_the_stack_and_a_fetch_is_due() {
	let bundled = descriptor(1, "first", PEOPLE);
	let mut source = load(&bundled, None);

	assert_eq!((people(&source), source.status()), (PEOPLE, status(&bundled, None)));
	let request = source.due_fetch(START).expect("a build copy is due at once");
	assert_eq!(
		(request.url.as_str(), request.if_none_match),
		("https://stack.test.invalid/stack.json", None)
	);
	assert_eq!(source.due_fetch(START), None, "one fetch at a time");
}

/// An operator that moves an endpoint publishes a new sequence. Nothing the
/// core built its services from changes, so the host reads the new copy
/// without a restart.
#[test]
fn a_newer_copy_on_the_same_chains_is_used_at_once() {
	let mut source = load(&descriptor(1, "first", PEOPLE), None);
	let newer = descriptor(2, "first", PEOPLE);

	let record = serve(&mut source, &newer, START);

	assert!(record.is_some(), "the newer copy is stored");
	assert_eq!(source.status(), status(&newer, Some(StackRefreshOutcome::Current)));
}

/// After a wipe the chains have new genesis hashes. The core's services were
/// built on the old ones, so it keeps running on them, tells the host to
/// restart, and starts from the new copy next time.
#[test]
fn a_restarted_stack_is_stored_and_runs_from_the_next_start() {
	let bundled = descriptor(1, "first", PEOPLE);
	let mut source = load(&bundled, None);

	let record = serve(&mut source, &descriptor(2, "second", WIPED_PEOPLE), START)
		.expect("the newer copy is stored");

	assert_eq!(
		(people(&source), source.status()),
		(
			PEOPLE,
			StackStatus {
				restart_required: true,
				new_generation: true,
				..status(&bundled, Some(StackRefreshOutcome::Current))
			}
		)
	);
	assert_eq!(people(&load(&bundled, Some(&record))), WIPED_PEOPLE);
}

/// A failed fetch only delays a change. The core keeps the copy it has and
/// the host can show that the stack is unreachable.
#[test]
fn an_unreachable_server_keeps_the_copy_in_use() {
	let bundled = descriptor(1, "first", PEOPLE);
	let mut source = load(&bundled, None);

	let record = source.fetched(Served::Unreachable, START);

	assert_eq!(
		(record, source.status()),
		(None, status(&bundled, Some(StackRefreshOutcome::Unreachable)))
	);
}

/// A signed descriptor without a Bulletin chain passes the descriptor rules
/// but cannot start a host. Storing it would break the next start, so it is
/// refused like any other bad copy.
#[test]
fn a_copy_the_core_cannot_run_is_refused_and_not_stored() {
	let bundled = descriptor(1, "first", PEOPLE);
	let mut source = load(&bundled, None);
	let mut without_bulletin = descriptor(2, "first", PEOPLE);
	without_bulletin["chains"]
		.as_array_mut()
		.expect("chains is an array")
		.retain(|chain| chain["role"] != "Bulletin");

	let record = serve(&mut source, &without_bulletin, START);

	assert_eq!(
		(record, source.status()),
		(
			None,
			status(
				&bundled,
				Some(StackRefreshOutcome::Refused {
					reason: "stack has no Bulletin chain".to_string()
				})
			)
		)
	);
}

/// A lower `sequence` under the stack's key is an old copy served again. It
/// must not take the stack back to chains that no longer exist.
#[test]
fn an_older_copy_is_refused() {
	let bundled = descriptor(2, "first", PEOPLE);
	let mut source = load(&bundled, None);

	let record = serve(&mut source, &descriptor(1, "first", WIPED_PEOPLE), START);

	assert_eq!(
		(record, source.status()),
		(
			None,
			status(&bundled, Some(StackRefreshOutcome::Refused { reason: "rollback".to_string() }))
		)
	);
}

/// A record written by another core version may hold a copy this one cannot
/// run. The bundled copy still starts the host.
#[test]
fn a_stored_copy_the_core_cannot_run_is_ignored() {
	let bundled = descriptor(1, "first", PEOPLE);
	let (link, _) = sign_test_descriptor(&bundled);
	let pointer = signed_pointer(&link).expect("test link carries a key");
	let mut on_dot = descriptor(2, "first", PEOPLE);
	on_dot["tld"] = "dot".into();
	let mut reader = StackReader::new(Refresh::Never);
	let record = reader
		.fetched(&pointer, Served::Document { bytes: &signed(&on_dot), etag: None }, START)
		.expect("the copy passes the descriptor rules")
		.stored
		.expect("a first copy is stored");

	let mut source = load(&bundled, Some(&record));

	assert_eq!(source.status(), status(&bundled, None));
	assert!(source.due_fetch(START).is_some(), "the bundled copy is due at once");
}

/// The descriptor's server sees one request an hour from each core, and a
/// host can still ask for one sooner.
#[test]
fn a_fetch_is_due_an_hour_after_the_last_one() {
	let mut source = load(&descriptor(1, "first", PEOPLE), None);
	source.due_fetch(START).expect("a build copy is due at once");
	serve(&mut source, &descriptor(2, "first", PEOPLE), START);

	assert_eq!(source.due_fetch(START + 3599), None);
	assert!(source.forced_fetch(START + 3599).is_some(), "a host may ask sooner");
	source.fetched(Served::Unreachable, START + 3599);
	assert_eq!(source.due_fetch(START + 3599 + 3599), None, "a failed fetch also waits an hour");
	assert!(source.due_fetch(START + 3599 + 3600).is_some());
}

/// A copy past `validUntil` or `expiresAt` still runs. The host is told, so
/// it can show the stack as stale or shut down.
#[test]
fn a_stale_or_expired_copy_is_reported() {
	let mut source = load(&descriptor(1, "first", PEOPLE), None);
	let mut newer = descriptor(2, "first", PEOPLE);
	newer["validUntil"] = (START - 10).into();
	newer["expiresAt"] = (START + 10).into();

	serve(&mut source, &newer, START);
	let stale = source.status();
	source.fetched(Served::Unreachable, START + 10);

	assert_eq!(
		[stale, source.status()],
		[
			StackStatus { stale: true, ..status(&newer, Some(StackRefreshOutcome::Current)) },
			StackStatus {
				stale: true,
				expired: true,
				..status(&newer, Some(StackRefreshOutcome::Unreachable))
			},
		]
	);
}
