use trinity_stack::{FetchRequest, Served};

use super::*;
use crate::{
	stack::signed_pointer,
	test_support::{
		TEST_STACK_PEOPLE, TEST_STACK_WIPED_PEOPLE, on_testnet, published_test_descriptor,
		sign_test_descriptor, stack_status, stored_test_record, without_bulletin,
	},
};

const START: i64 = 1_800_000_000;

/// A connection opened for People that reached the chain serving its endpoint
/// after a wipe.
const PEOPLE_WIPED: GenesisHashes =
	GenesisHashes { named: TEST_STACK_PEOPLE, answered: TEST_STACK_WIPED_PEOPLE };

fn signed(descriptor: &serde_json::Value) -> Vec<u8> {
	sign_test_descriptor(descriptor).1
}

fn load(bundled: &serde_json::Value, stored: Option<&[u8]>) -> StackSource {
	load_reserving(bundled, stored, ReservedValuesPolicy::default())
}

fn load_reserving(
	bundled: &serde_json::Value,
	stored: Option<&[u8]>,
	reserved_values: ReservedValuesPolicy,
) -> StackSource {
	let (link, bytes) = sign_test_descriptor(bundled);
	let pointer = signed_pointer(&link).expect("test link carries a key");
	StackSource::load(pointer, &bytes, stored, reserved_values, START)
		.expect("bundled copy is valid")
}

/// A build that reserves the `testnet` TLD and one more genesis hash.
fn reserving_extras() -> ReservedValuesPolicy {
	ReservedValuesPolicy {
		extra_genesis_hashes: vec![EXTRA_RESERVED_GENESIS],
		extra_tlds: vec!["testnet".to_string()],
		..ReservedValuesPolicy::default()
	}
}

const EXTRA_RESERVED_GENESIS: [u8; 32] = [0xdd; 32];

fn serve(source: &mut StackSource, descriptor: &serde_json::Value, now: i64) -> Option<Vec<u8>> {
	let bytes = signed(descriptor);
	source.fetched(Ok(Served::Document { bytes: &bytes, etag: None }), now)
}

/// [`serve`], with the record it returns stored, as the refresh loop stores
/// it before the host is told of the fetch.
fn serve_stored(
	source: &mut StackSource,
	descriptor: &serde_json::Value,
	now: i64,
) -> Option<Vec<u8>> {
	let record = serve(source, descriptor, now);
	if record.is_some() {
		source.record_stored();
	}
	record
}

fn unreachable() -> StackRefreshOutcome {
	StackRefreshOutcome::Unreachable { reason: "connection refused".to_string() }
}

/// A fetch that failed at `now`.
fn fail(source: &mut StackSource, now: i64) -> Option<Vec<u8>> {
	source.fetched(Err("connection refused".to_string()), now)
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
		(TEST_STACK_PEOPLE, stack_status(&bundled, None))
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

	let record = serve_stored(&mut source, &newer, START);

	assert!(record.is_some(), "the newer copy is stored");
	assert_eq!(source.status(), stack_status(&newer, Some(StackRefreshOutcome::Current)));
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
				..stack_status(&bundled, Some(StackRefreshOutcome::Current))
			}
		)
	);
	// An app killed before it restarts gets no second notice. The status at
	// its next start still names the new generation, so the host can tell
	// its cached chain data is for the old chains. The stored copy is the
	// same stack under its own generation.
	let restarted = load(&bundled, Some(&record));
	assert_eq!(
		(
			restarted.stack().chains().people,
			restarted.stack().id(),
			restarted.stack().generation(),
			restarted.status()
		),
		(TEST_STACK_WIPED_PEOPLE, source.stack().id(), "second", stack_status(&wiped, None))
	);
}

/// A new generation on the same genesis hashes still means the chains
/// restarted, so the core asks for a restart rather than switching to it.
#[test]
fn a_new_generation_on_the_same_chains_runs_from_the_next_start() {
	let bundled = published_test_descriptor(1, "first", TEST_STACK_PEOPLE);
	let mut source = load(&bundled, None);

	serve(&mut source, &published_test_descriptor(2, "second", TEST_STACK_PEOPLE), START)
		.expect("the newer copy is stored");
	source.record_stored();

	assert_eq!(
		(source.stack().generation(), source.status()),
		(
			"first",
			StackStatus {
				restart_required: true,
				..stack_status(&bundled, Some(StackRefreshOutcome::Current))
			}
		)
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
	source.record_not_stored("disk full".to_string());
	let after_the_failed_store = source.status().restart_required;
	let second_record = serve(&mut source, &wiped, START + 3600);
	source.record_stored();

	assert_eq!(
		(
			first_record.is_some(),
			after_the_failed_store,
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

	assert_eq!((record, source.status()), (None, stack_status(&bundled, Some(unreachable()))));
}

/// A signed descriptor without a Bulletin chain passes the descriptor rules
/// but cannot start a host. Storing it would break the next start, so it is
/// refused like any other bad copy.
#[test]
fn a_copy_the_core_cannot_run_is_refused_and_not_stored() {
	let bundled = published_test_descriptor(1, "first", TEST_STACK_PEOPLE);
	let mut source = load(&bundled, None);
	let without_bulletin =
		without_bulletin(published_test_descriptor(2, "first", TEST_STACK_PEOPLE));

	let record = serve(&mut source, &without_bulletin, START);

	assert_eq!(
		(record, source.status()),
		(
			None,
			stack_status(
				&bundled,
				Some(StackRefreshOutcome::Refused {
					reason: "stack has no Bulletin chain".to_string()
				})
			)
		)
	);
}

/// The build's own reserved values hold for every copy it runs, not only the
/// bundled one, so a newer copy that names one is refused and not stored.
#[test]
fn a_refreshed_copy_that_names_an_extra_reserved_value_is_refused() {
	let bundled = published_test_descriptor(1, "first", TEST_STACK_PEOPLE);
	let mut on_extra_tld = published_test_descriptor(2, "first", TEST_STACK_PEOPLE);
	on_extra_tld["tld"] = "testnet".into();
	let on_extra_genesis = published_test_descriptor(2, "first", EXTRA_RESERVED_GENESIS);
	let refused = |reason: String| {
		(None, stack_status(&bundled, Some(StackRefreshOutcome::Refused { reason })))
	};

	assert_eq!(
		[&on_extra_tld, &on_extra_genesis].map(|newer| {
			let mut source = load_reserving(&bundled, None, reserving_extras());
			(serve(&mut source, newer, START), source.status())
		}),
		[
			refused("stack uses the reserved tld testnet, which this build does not allow".into()),
			refused(format!(
				"stack uses the reserved genesis hash 0x{}, which this build does not allow",
				hex::encode(EXTRA_RESERVED_GENESIS)
			)),
		]
	);
}

/// A copy stored before the build reserved one of its values is not run: the
/// bundled copy starts the host.
#[test]
fn a_stored_copy_that_names_an_extra_reserved_value_is_ignored() {
	let bundled = published_test_descriptor(1, "first", TEST_STACK_PEOPLE);
	let (link, _) = sign_test_descriptor(&bundled);
	let pointer = signed_pointer(&link).expect("test link carries a key");
	let on_extra_genesis = published_test_descriptor(2, "first", EXTRA_RESERVED_GENESIS);
	let mut reader = StackReader::new(Refresh::Never);
	let record = reader
		.fetched(
			&pointer,
			Served::Document { bytes: &signed(&on_extra_genesis), etag: None },
			START,
		)
		.expect("the copy passes the descriptor rules")
		.stored
		.expect("a first copy is stored");

	let source = load_reserving(&bundled, Some(&record), reserving_extras());

	assert_eq!(source.status(), stack_status(&bundled, None));
}

/// A fresh install starts from the bundled copy, so a build whose policy
/// refuses that copy cannot run there. A device that holds a newer stored copy
/// is refused the same way, so the mistake shows the first time the build
/// runs anywhere rather than only on new installs.
#[test]
fn a_bundled_copy_the_policy_refuses_fails_the_start_with_or_without_a_stored_copy() {
	let bundled = published_test_descriptor(1, "first", EXTRA_RESERVED_GENESIS);
	let (link, bytes) = sign_test_descriptor(&bundled);
	let pointer = signed_pointer(&link).expect("test link carries a key");
	let newer = stored_test_record(&published_test_descriptor(2, "first", TEST_STACK_PEOPLE), 0);

	let start = |stored, reserved_values| {
		StackSource::load(pointer.clone(), &bytes, stored, reserved_values, START).err()
	};
	let refused = || Some(StackError::ReservedGenesis { genesis_hash: EXTRA_RESERVED_GENESIS });

	assert_eq!(
		[
			start(None, reserving_extras()),
			start(Some(newer.as_slice()), reserving_extras()),
			start(Some(newer.as_slice()), ReservedValuesPolicy::default()),
		],
		[refused(), refused(), None],
		"the policy refuses the bundled copy on any device, and the stored copy is one it takes"
	);
}

/// A copy the core refuses must not become the copy the reader judges fetches
/// against, or the corrected descriptor at the same `sequence` would be
/// refused as a conflict.
#[test]
fn a_refused_copy_does_not_stand_in_the_way_of_the_corrected_one() {
	let corrected = published_test_descriptor(2, "first", TEST_STACK_PEOPLE);
	let refused = [without_bulletin(corrected.clone()), on_testnet(corrected.clone())];

	let fetched = refused.map(|copy| {
		let mut source = load(&published_test_descriptor(1, "first", TEST_STACK_PEOPLE), None);
		let refusal = serve(&mut source, &copy, START);
		let record = serve_stored(&mut source, &corrected, START + 3600);
		(refusal, record.is_some(), source.status())
	});

	let taken = (None, true, stack_status(&corrected, Some(StackRefreshOutcome::Current)));
	assert_eq!(fetched, [taken.clone(), taken]);
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
	let record =
		serve(&mut source, &published_test_descriptor(2, "first", TEST_STACK_PEOPLE), START)
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
			stack_status(
				&bundled,
				Some(StackRefreshOutcome::Refused { reason: "rollback".to_string() })
			)
		)
	);
}

/// The first fetch of a core running its bundled copy: due at once, with no
/// `ETag`.
fn bundled_copys_fetch() -> Option<FetchRequest> {
	Some(FetchRequest {
		url: "https://stack.test.invalid/stack.json".to_string(),
		if_none_match: None,
	})
}

/// A record written by another core version or another build may hold a copy
/// this core refuses. The bundled copy starts the host and is fetched at once,
/// though the record is fresh.
#[test]
fn a_stored_copy_the_core_refuses_is_ignored() {
	let bundled = published_test_descriptor(1, "first", TEST_STACK_PEOPLE);
	let newer = published_test_descriptor(2, "first", TEST_STACK_PEOPLE);
	let refused = [without_bulletin(newer.clone()), on_testnet(newer)];

	let started = refused.map(|copy| {
		let mut source = load(&bundled, Some(&stored_test_record(&copy, START)));
		(source.status(), source.due_fetch(START))
	});

	let from_bundled = (stack_status(&bundled, None), bundled_copys_fetch());
	assert_eq!(started, [from_bundled.clone(), from_bundled]);
}

/// A `304 Not Modified` tells the core its copy is current. Sending an ignored
/// record's `ETag` would get that answer for the bundled copy.
#[test]
fn an_ignored_records_etag_is_not_sent() {
	let bundled = published_test_descriptor(1, "first", TEST_STACK_PEOPLE);
	let newer = published_test_descriptor(2, "first", TEST_STACK_PEOPLE);
	let fetched_long_ago = START - 2 * 3600;
	let ignored = [without_bulletin(newer.clone()), on_testnet(newer)];

	assert_eq!(
		ignored.map(|ignored| {
			let record = stored_test_record(&ignored, fetched_long_ago);
			load(&bundled, Some(&record)).due_fetch(START)
		}),
		[bundled_copys_fetch(), bundled_copys_fetch()]
	);
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

	serve_stored(&mut source, &newer, START);
	let stale = source.status();
	let _no_record = fail(&mut source, START + 10);

	assert_eq!(
		[stale, source.status()],
		[
			StackStatus { stale: true, ..stack_status(&newer, Some(StackRefreshOutcome::Current)) },
			StackStatus { stale: true, expired: true, ..stack_status(&newer, Some(unreachable())) },
		]
	);
}

/// A copy the core fetched at [`START`], and the source that holds it.
fn fetched_at_start() -> (serde_json::Value, StackSource) {
	let bundled = published_test_descriptor(1, "first", TEST_STACK_PEOPLE);
	let mut source = load(&bundled, None);
	source.due_fetch(START).expect("a build copy is due at once");
	serve_stored(&mut source, &bundled, START);
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

	let listing = source.chain_answered(PEOPLE_WIPED);

	assert_eq!(
		(
			listing,
			source.status(),
			hourly,
			source.next_fetch_in(START + 60),
			source.due_fetch(START + 299),
			source.due_fetch(START + 300).is_some(),
		),
		(
			Listing::Listed(people_gave(TEST_STACK_WIPED_PEOPLE)),
			StackStatus {
				mismatches: vec![people_gave(TEST_STACK_WIPED_PEOPLE)],
				..stack_status(&bundled, Some(StackRefreshOutcome::Current))
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
	source.chain_answered(PEOPLE_WIPED);
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
	let listings = answers.map(|answered| {
		source.chain_answered(GenesisHashes { named: TEST_STACK_PEOPLE, answered })
	});

	assert_eq!(
		(listings, source.due_fetch(START + 299), source.status().mismatches),
		(
			[
				Listing::Listed(people_gave(TEST_STACK_WIPED_PEOPLE)),
				Listing::Unchanged,
				Listing::Cleared,
				Listing::Listed(people_gave([0x55; 32])),
			],
			None,
			vec![people_gave([0x55; 32])]
		)
	);
}

/// A wipe restarts every chain of a stack, and their connections answer in
/// any order and more than once. The host is shown each chain once, in the
/// descriptor's order and with the hash it gave last, and a chain that
/// answers as named again leaves the others listed.
#[test]
fn chains_that_restarted_together_are_each_listed_once_in_the_descriptors_order() {
	let (_bundled, mut source) = fetched_at_start();
	let (asset_hub, bulletin) = ([0xcc; 32], [0xbb; 32]);
	let gave = |role: &str, named, answered| StackMismatch::GenesisHash {
		role: role.to_string(),
		named,
		answered,
	};

	source.chain_answered(GenesisHashes { named: bulletin, answered: [0xb1; 32] });
	source.chain_answered(PEOPLE_WIPED);
	source.chain_answered(GenesisHashes { named: asset_hub, answered: [0xc1; 32] });
	let listing = source.chain_answered(GenesisHashes { named: bulletin, answered: [0xb2; 32] });
	let all_wrong = source.status().mismatches;
	source.chain_answered(GenesisHashes { named: asset_hub, answered: asset_hub });

	assert_eq!(
		(listing, all_wrong, source.status().mismatches),
		(
			Listing::Listed(gave("Bulletin", bulletin, [0xb2; 32])),
			vec![
				gave("AssetHub", asset_hub, [0xc1; 32]),
				people_gave(TEST_STACK_WIPED_PEOPLE),
				gave("Bulletin", bulletin, [0xb2; 32]),
			],
			vec![people_gave(TEST_STACK_WIPED_PEOPLE), gave("Bulletin", bulletin, [0xb2; 32])],
		)
	);
}

/// The core logs a mismatch as it lists it, and a native host is handed that
/// warning as text. It has to say which chain and give both hashes.
#[test]
fn a_mismatch_reads_as_the_warning_the_core_logs() {
	assert_eq!(
		people_gave(TEST_STACK_WIPED_PEOPLE).to_string(),
		format!(
			"the People chain gave genesis hash 0x{}, not 0x{}",
			"a9".repeat(32),
			"a1".repeat(32)
		)
	);
}

/// A host may serve chains the stack does not name, such as another network
/// a product asks for. Fetching the stack's descriptor again would tell
/// nothing about them.
#[test]
fn a_chain_outside_the_stack_is_not_the_stacks_mismatch() {
	let (bundled, mut source) = fetched_at_start();

	let listing = source.chain_answered(GenesisHashes { named: [0x77; 32], answered: [0x78; 32] });

	assert_eq!(
		(listing, source.status(), source.next_fetch_in(START)),
		(
			Listing::Unchanged,
			stack_status(&bundled, Some(StackRefreshOutcome::Current)),
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
	source.chain_answered(PEOPLE_WIPED);

	let listing = source
		.chain_answered(GenesisHashes { named: TEST_STACK_PEOPLE, answered: TEST_STACK_PEOPLE });

	assert_eq!(
		(listing, source.status(), source.next_fetch_in(START)),
		(
			Listing::Cleared,
			stack_status(&bundled, Some(StackRefreshOutcome::Current)),
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
	source.chain_answered(PEOPLE_WIPED);
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
				..stack_status(&bundled, Some(StackRefreshOutcome::Current))
			},
			Some(Duration::from_secs(300)),
			Some(Duration::from_secs(3600)),
		)
	);
}
