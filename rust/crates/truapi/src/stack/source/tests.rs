use trinity_stack::{Descriptor, Refresh, Served, StackReader};

use super::*;
use crate::{
	stack::signed_pointer,
	test_support::{
		TEST_STACK_PEOPLE, TEST_STACK_WIPED_PEOPLE, published_test_descriptor, sign_test_descriptor,
	},
};

const START: i64 = 1_800_000_000;

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
	source.fetched(Ok(Served::Document { bytes: &bytes, etag: None }), now)
}

fn unreachable() -> StackRefreshOutcome {
	StackRefreshOutcome::Unreachable { reason: "connection refused".to_string() }
}

/// A fetch that failed at `now`.
fn fail(source: &mut StackSource, now: i64) -> Option<Vec<u8>> {
	source.fetched(Err("connection refused".to_string()), now)
}

fn status(
	descriptor: &serde_json::Value,
	last_refresh: Option<StackRefreshOutcome>,
) -> StackStatus {
	StackStatus {
		descriptor: plain(descriptor),
		generation: descriptor["generation"].as_str().expect("generation is a string").to_string(),
		stale: false,
		expired: false,
		restart_required: false,
		last_refresh,
		mismatches: Vec::new(),
	}
}

fn people_gave(answered: [u8; 32]) -> StackMismatch {
	StackMismatch::GenesisHash { role: "People".to_string(), named: TEST_STACK_PEOPLE, answered }
}

/// A stack never waits on its descriptor's server: the bundled copy is enough
/// to start, and the first fetch is only asked for.
#[test]
fn the_bundled_copy_starts_the_stack_and_a_fetch_is_due() {
	let bundled = published_test_descriptor(1, "first", TEST_STACK_PEOPLE);
	let mut source = load(&bundled, None);

	assert_eq!(
		(source.stack().chains().people, source.status()),
		(TEST_STACK_PEOPLE, status(&bundled, None))
	);
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
	let mut source = load(&published_test_descriptor(1, "first", TEST_STACK_PEOPLE), None);
	let newer = published_test_descriptor(2, "first", TEST_STACK_PEOPLE);

	let record = serve(&mut source, &newer, START);

	assert!(record.is_some(), "the newer copy is stored");
	assert_eq!(source.status(), status(&newer, Some(StackRefreshOutcome::Current)));
}

/// After a wipe the chains have new genesis hashes. The core's services were
/// built on the old ones, so it keeps running on them, tells the host to
/// restart, and starts from the new copy next time.
#[test]
fn a_restarted_stack_is_stored_and_runs_from_the_next_start() {
	let bundled = published_test_descriptor(1, "first", TEST_STACK_PEOPLE);
	let wiped = published_test_descriptor(2, "second", TEST_STACK_WIPED_PEOPLE);
	let mut source = load(&bundled, None);

	let record = serve(&mut source, &wiped, START).expect("the newer copy is stored");
	source.record_stored();

	assert_eq!(
		(source.stack().chains().people, source.status()),
		(
			TEST_STACK_PEOPLE,
			StackStatus {
				restart_required: true,
				..status(&bundled, Some(StackRefreshOutcome::Current))
			}
		)
	);
	// An app killed before it restarts gets no second notice. The status at
	// its next start still names the new generation, so the host can tell
	// its cached chain data is for the old chains.
	let restarted = load(&bundled, Some(&record));
	assert_eq!(
		(restarted.stack().chains().people, restarted.status()),
		(TEST_STACK_WIPED_PEOPLE, status(&wiped, None))
	);
}

/// A host told to restart before the record is stored would start from the
/// old copy, fetch the new one and be told to restart again, for as long as
/// the store fails.
#[test]
fn a_restart_is_asked_for_only_once_the_record_is_stored() {
	let bundled = published_test_descriptor(1, "first", TEST_STACK_PEOPLE);
	let wiped = published_test_descriptor(2, "second", TEST_STACK_WIPED_PEOPLE);
	let mut source = load(&bundled, None);

	let first_record = serve(&mut source, &wiped, START);
	let before_the_store = source.status().restart_required;
	let second_record = serve(&mut source, &wiped, START + 3600);
	source.record_stored();

	assert_eq!(
		(
			first_record.is_some(),
			before_the_store,
			second_record.is_some(),
			source.status().restart_required
		),
		(true, false, true, true),
		"a later fetch hands the record over again, for a store that failed"
	);
}

/// A failed fetch only delays a change. The core keeps the copy it has and
/// the host can show that the stack is unreachable.
#[test]
fn an_unreachable_server_keeps_the_copy_in_use() {
	let bundled = published_test_descriptor(1, "first", TEST_STACK_PEOPLE);
	let mut source = load(&bundled, None);

	let record = fail(&mut source, START);

	assert_eq!((record, source.status()), (None, status(&bundled, Some(unreachable()))));
}

/// A signed descriptor without a Bulletin chain passes the descriptor rules
/// but cannot start a host. Storing it would break the next start, so it is
/// refused like any other bad copy.
#[test]
fn a_copy_the_core_cannot_run_is_refused_and_not_stored() {
	let bundled = published_test_descriptor(1, "first", TEST_STACK_PEOPLE);
	let mut source = load(&bundled, None);
	let mut without_bulletin = published_test_descriptor(2, "first", TEST_STACK_PEOPLE);
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

/// The reader judges every fetch against the copy it holds. A copy the core
/// refused must not become that copy, or the operator's corrected descriptor
/// at the same `sequence` would be turned away as a conflict.
#[test]
fn a_refused_copy_does_not_stand_in_the_way_of_the_corrected_one() {
	let bundled = published_test_descriptor(1, "first", TEST_STACK_PEOPLE);
	let mut source = load(&bundled, None);
	let corrected = published_test_descriptor(2, "first", TEST_STACK_PEOPLE);
	let mut on_dot = corrected.clone();
	on_dot["tld"] = "dot".into();

	let refused = serve(&mut source, &on_dot, START);
	let record = serve(&mut source, &corrected, START + 3600);

	assert_eq!(
		(refused, record.is_some(), source.status()),
		(None, true, status(&corrected, Some(StackRefreshOutcome::Current)))
	);
}

/// While a host rebuilds its runtime, the old and the new one can both
/// refresh. The slower one must not store an older copy over a newer one, or
/// the next start would go back to chains that no longer exist.
#[test]
fn a_record_never_replaces_a_newer_one() {
	let mut source = load(&published_test_descriptor(1, "first", TEST_STACK_PEOPLE), None);
	let record_of = |sequence| {
		let mut source = load(&published_test_descriptor(1, "first", TEST_STACK_PEOPLE), None);
		serve(&mut source, &published_test_descriptor(sequence, "first", TEST_STACK_PEOPLE), START)
			.expect("a newer copy is stored")
	};
	let record = serve(&mut source, &published_test_descriptor(2, "first", TEST_STACK_PEOPLE), START)
		.expect("a newer copy is stored");
	let check = source.replace_check(&record);

	assert_eq!(
		[
			check.replaces(&record_of(3)),
			check.replaces(&record_of(2)),
			check.replaces(b"not a record")
		],
		[false, true, true]
	);
}

/// A lower `sequence` under the stack's key is an old copy served again. It
/// must not take the stack back to chains that no longer exist.
#[test]
fn an_older_copy_is_refused() {
	let bundled = published_test_descriptor(2, "first", TEST_STACK_PEOPLE);
	let mut source = load(&bundled, None);

	let record =
		serve(&mut source, &published_test_descriptor(1, "first", TEST_STACK_WIPED_PEOPLE), START);

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
	let bundled = published_test_descriptor(1, "first", TEST_STACK_PEOPLE);
	let (link, _) = sign_test_descriptor(&bundled);
	let pointer = signed_pointer(&link).expect("test link carries a key");
	let mut on_dot = published_test_descriptor(2, "first", TEST_STACK_PEOPLE);
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
	let mut source = load(&published_test_descriptor(1, "first", TEST_STACK_PEOPLE), None);
	source.due_fetch(START).expect("a build copy is due at once");
	serve(&mut source, &published_test_descriptor(2, "first", TEST_STACK_PEOPLE), START);

	assert_eq!(source.due_fetch(START + 3599), None);
	assert!(source.forced_fetch(START + 3599).is_some(), "a host may ask sooner");
	let _no_record = fail(&mut source, START + 3599);
	assert_eq!(source.due_fetch(START + 3599 + 3599), None, "a failed fetch also waits an hour");
	assert!(source.due_fetch(START + 3599 + 3600).is_some());
}

/// The refresh loop sleeps for as long as this says. An answer that came
/// early would make it spin, and one that came late would delay a wipe.
#[test]
fn the_wait_for_the_next_fetch_counts_down_to_the_hour() {
	let mut source = load(&published_test_descriptor(1, "first", TEST_STACK_PEOPLE), None);
	let before_first_fetch = source.next_fetch_in(START);
	source.due_fetch(START).expect("a build copy is due at once");
	let while_fetching = source.next_fetch_in(START);
	serve(&mut source, &published_test_descriptor(2, "first", TEST_STACK_PEOPLE), START);

	assert_eq!(
		[
			before_first_fetch,
			while_fetching,
			source.next_fetch_in(START + 600),
			source.next_fetch_in(START + 3600),
		],
		[Some(Duration::ZERO), None, Some(Duration::from_secs(3000)), Some(Duration::ZERO)]
	);
}

/// A copy past `validUntil` or `expiresAt` still runs. The host is told, so
/// it can show the stack as stale or shut down.
#[test]
fn a_stale_or_expired_copy_is_reported() {
	let mut source = load(&published_test_descriptor(1, "first", TEST_STACK_PEOPLE), None);
	let mut newer = published_test_descriptor(2, "first", TEST_STACK_PEOPLE);
	newer["validUntil"] = (START - 10).into();
	newer["expiresAt"] = (START + 10).into();

	serve(&mut source, &newer, START);
	let stale = source.status();
	let _no_record = fail(&mut source, START + 10);

	assert_eq!(
		[stale, source.status()],
		[
			StackStatus { stale: true, ..status(&newer, Some(StackRefreshOutcome::Current)) },
			StackStatus { stale: true, expired: true, ..status(&newer, Some(unreachable())) },
		]
	);
}

/// A copy the core fetched at [`START`], and the source that holds it.
fn fetched_at_start() -> (serde_json::Value, StackSource) {
	let bundled = published_test_descriptor(1, "first", TEST_STACK_PEOPLE);
	let mut source = load(&bundled, None);
	source.due_fetch(START).expect("a build copy is due at once");
	serve(&mut source, &bundled, START);
	(bundled, source)
}

/// After a wipe the node behind People's endpoint serves a new chain. The
/// host is told which chain it is. The operator publishes the new genesis
/// hashes with the wipe, so the descriptor is fetched once the copy is five
/// minutes old rather than an hour.
#[test]
fn a_chain_that_gave_a_different_genesis_hash_is_reported_and_brings_the_fetch_forward() {
	let (bundled, mut source) = fetched_at_start();
	let hourly = source.next_fetch_in(START + 60);

	let changed = source.chain_answered(TEST_STACK_PEOPLE, TEST_STACK_WIPED_PEOPLE);

	assert_eq!(
		(
			changed,
			source.status(),
			hourly,
			source.next_fetch_in(START + 60),
			source.due_fetch(START + 299),
			source.due_fetch(START + 300).is_some(),
		),
		(
			true,
			StackStatus {
				mismatches: vec![people_gave(TEST_STACK_WIPED_PEOPLE)],
				..status(&bundled, Some(StackRefreshOutcome::Current))
			},
			Some(Duration::from_secs(3540)),
			Some(Duration::from_secs(240)),
			None,
			true,
		)
	);
}

/// An operator may publish the new descriptor some time after the wipe. A
/// single fetch would leave the chain out of use until the hourly one, so
/// the descriptor is asked for every five minutes while the chain is wrong.
#[test]
fn a_chain_that_stays_wrong_is_asked_about_every_five_minutes() {
	let (bundled, mut source) = fetched_at_start();
	source.chain_answered(TEST_STACK_PEOPLE, TEST_STACK_WIPED_PEOPLE);
	source.due_fetch(START + 300).expect("the copy is five minutes old");

	serve(&mut source, &bundled, START + 300);

	assert_eq!(
		[source.next_fetch_in(START + 300), source.next_fetch_in(START + 600)],
		[Some(Duration::from_secs(300)), Some(Duration::ZERO)]
	);
}

/// Every connection to a wiped chain gets a wrong answer, the core's services
/// retry their connections, and a broken endpoint may give a new hash each
/// time. None of that reaches the descriptor's server more than once in
/// five minutes.
#[test]
fn wrong_answers_do_not_bring_a_fetch_sooner_than_five_minutes() {
	let (_bundled, mut source) = fetched_at_start();

	let answers = [TEST_STACK_WIPED_PEOPLE, TEST_STACK_WIPED_PEOPLE, TEST_STACK_PEOPLE, [0x55; 32]];
	let changed = answers.map(|answered| source.chain_answered(TEST_STACK_PEOPLE, answered));

	assert_eq!(
		(changed, source.due_fetch(START + 299), source.status().mismatches),
		([true, false, true, true], None, vec![people_gave([0x55; 32])])
	);
}

/// A host may serve chains the stack does not name, such as another network
/// a product asks for. Fetching the stack's descriptor again would tell
/// nothing about them.
#[test]
fn a_chain_outside_the_stack_is_not_the_stacks_mismatch() {
	let (bundled, mut source) = fetched_at_start();

	let changed = source.chain_answered([0x77; 32], [0x78; 32]);

	assert_eq!(
		(changed, source.status(), source.next_fetch_in(START)),
		(
			false,
			status(&bundled, Some(StackRefreshOutcome::Current)),
			Some(Duration::from_secs(3600))
		)
	);
}

/// One node behind an endpoint may lag a wipe while the others do not. Once
/// a connection reaches the chain the descriptor names, the host stops
/// showing the chain as wrong and the descriptor is fetched hourly again.
#[test]
fn a_chain_that_gives_its_own_genesis_hash_again_is_no_longer_reported() {
	let (bundled, mut source) = fetched_at_start();
	source.chain_answered(TEST_STACK_PEOPLE, TEST_STACK_WIPED_PEOPLE);

	let changed = source.chain_answered(TEST_STACK_PEOPLE, TEST_STACK_PEOPLE);

	assert_eq!(
		(changed, source.status(), source.next_fetch_in(START)),
		(
			true,
			status(&bundled, Some(StackRefreshOutcome::Current)),
			Some(Duration::from_secs(3600))
		)
	);
}

/// The published descriptor follows the wipe, so the host is told to restart.
/// Until it does, the core still runs the old chain, which still answers
/// wrong. The sooner fetches have brought what they were for, so the
/// descriptor's server is asked hourly again.
#[test]
fn a_mismatch_stays_reported_until_the_restart_the_newer_copy_asks_for() {
	let bundled = published_test_descriptor(1, "first", TEST_STACK_PEOPLE);
	let mut source = load(&bundled, None);
	source.chain_answered(TEST_STACK_PEOPLE, TEST_STACK_WIPED_PEOPLE);
	source.due_fetch(START).expect("a build copy is due at once");

	serve(&mut source, &published_test_descriptor(2, "second", TEST_STACK_WIPED_PEOPLE), START)
		.expect("the newer copy is stored");
	let before_it_is_stored = source.next_fetch_in(START);
	source.record_stored();

	assert_eq!(
		(source.status(), before_it_is_stored, source.next_fetch_in(START)),
		(
			StackStatus {
				restart_required: true,
				mismatches: vec![people_gave(TEST_STACK_WIPED_PEOPLE)],
				..status(&bundled, Some(StackRefreshOutcome::Current))
			},
			Some(Duration::from_secs(300)),
			Some(Duration::from_secs(3600)),
		)
	);
}

fn suffix_is(answered: &str) -> StackMismatch {
	StackMismatch::NetworkSuffix { named: "paseo".to_string(), answered: answered.to_string() }
}

/// The core derives identities under `peopl.<tld>` and builds proof contexts
/// with People's suffix, so an identity derived under one does not match the
/// other, and the host is told both values. Nothing is refused over a suffix,
/// so the descriptor's server is not asked any sooner.
#[test]
fn a_people_suffix_that_is_not_the_tld_is_reported() {
	let (bundled, mut source) = fetched_at_start();

	let changed = source.people_suffix_answered(b"testnet");

	assert_eq!(
		(changed, source.status(), source.next_fetch_in(START)),
		(
			true,
			StackStatus {
				mismatches: vec![suffix_is("testnet")],
				..status(&bundled, Some(StackRefreshOutcome::Current))
			},
			Some(Duration::from_secs(3600)),
		)
	);
}

/// The usual answer is the stack's own `tld`, and it changes nothing the host
/// is told.
#[test]
fn a_people_suffix_equal_to_the_tld_is_not_reported() {
	let bundled = published_test_descriptor(1, "first", TEST_STACK_PEOPLE);
	let mut source = load(&bundled, None);

	let changed = source.people_suffix_answered(b"paseo");

	assert_eq!((changed, source.status()), (false, status(&bundled, None)));
}

/// Every allowance flow reads the suffix again. Once People gives the `tld`,
/// as after its operator corrected it, the host stops showing it as wrong.
#[test]
fn a_people_suffix_read_as_the_tld_later_is_no_longer_reported() {
	let bundled = published_test_descriptor(1, "first", TEST_STACK_PEOPLE);
	let mut source = load(&bundled, None);
	source.people_suffix_answered(b"testnet");

	let changed = source.people_suffix_answered(b"paseo");

	assert_eq!((changed, source.status()), (true, status(&bundled, None)));
}

/// Every allowance flow reads the suffix again, so the same wrong answer
/// comes back each time. Only a different one is news to the host.
#[test]
fn a_wrong_people_suffix_read_again_changes_nothing_until_it_differs() {
	let mut source = load(&published_test_descriptor(1, "first", TEST_STACK_PEOPLE), None);
	source.people_suffix_answered(b"testnet");

	let again = source.people_suffix_answered(b"testnet");
	let another = source.people_suffix_answered(b"other");

	assert_eq!(
		(again, another, source.status().mismatches),
		(false, true, vec![suffix_is("other")])
	);
}

/// A host shows the chains it must keep off first, then the suffix, and the
/// status crosses to hosts as text whatever bytes People stores.
#[test]
fn the_suffix_is_listed_after_the_chains_and_as_text() {
	let mut source = load(&published_test_descriptor(1, "first", TEST_STACK_PEOPLE), None);

	source.people_suffix_answered(&[0xff, b'x']);
	source.chain_answered(TEST_STACK_PEOPLE, TEST_STACK_WIPED_PEOPLE);

	assert_eq!(
		source.status().mismatches,
		vec![people_gave(TEST_STACK_WIPED_PEOPLE), suffix_is("\u{fffd}x")]
	);
}
