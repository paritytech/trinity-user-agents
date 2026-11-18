use trinity_stack::{Descriptor, Refresh, Served, StackReader};

use super::*;
use crate::{
	stack::signed_pointer,
	test_support::{
		TEST_STACK_PEOPLE, TEST_STACK_WIPED_PEOPLE, published_test_descriptor,
		reserved_test_descriptor, sign_test_descriptor,
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
	}
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

/// The build's own reserved values hold for every copy it runs, not only the
/// bundled one, so a newer copy that names one is refused and not stored.
#[test]
fn a_refreshed_copy_that_names_an_extra_reserved_value_is_refused() {
	let bundled = published_test_descriptor(1, "first", TEST_STACK_PEOPLE);
	let mut on_extra_tld = published_test_descriptor(2, "first", TEST_STACK_PEOPLE);
	on_extra_tld["tld"] = "testnet".into();
	let on_extra_genesis = published_test_descriptor(2, "first", EXTRA_RESERVED_GENESIS);
	let refused =
		|reason: String| (None, status(&bundled, Some(StackRefreshOutcome::Refused { reason })));

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

	assert_eq!(source.status(), status(&bundled, None));
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

/// The published copy `descriptor`, moved to the `testnet` TLD.
fn on_testnet(mut descriptor: serde_json::Value) -> serde_json::Value {
	descriptor["tld"] = "testnet".into();
	descriptor
}

fn refused_for_its_tld() -> Option<StackRefreshOutcome> {
	refused_for_pinned("tld")
}

fn refused_for_pinned(field: &str) -> Option<StackRefreshOutcome> {
	Some(StackRefreshOutcome::Refused {
		reason: format!("stack descriptor changes {field}, which the bundled copy pins"),
	})
}

/// Accounts derive under the TLD, so a descriptor signed with a stolen key
/// must not move a stack to another one. The copy is refused on every fetch
/// and never stored, and the core keeps the copy it holds.
#[test]
fn a_refreshed_copy_on_another_tld_is_refused_and_not_stored() {
	let mut source = load(&published_test_descriptor(1, "first", TEST_STACK_PEOPLE), None);
	let held = published_test_descriptor(2, "first", TEST_STACK_PEOPLE);
	let moved = on_testnet(published_test_descriptor(3, "first", TEST_STACK_PEOPLE));
	let held_record = serve(&mut source, &held, START);

	let refusals = [START + 3600, START + 7200].map(|now| {
		(serve(&mut source, &moved, now), source.status(), source.stack().tld() == "paseo")
	});

	let refused = (None, status(&held, refused_for_its_tld()), true);
	assert_eq!((held_record.is_some(), refusals), (true, [refused.clone(), refused]));
}

/// The reader judges every fetch against the copy it holds. A copy refused
/// for its TLD must not become that copy, or the operator's corrected
/// descriptor at the same `sequence` would be turned away as a conflict.
#[test]
fn a_copy_refused_for_its_tld_does_not_stand_in_the_way_of_the_corrected_one() {
	let mut source = load(&published_test_descriptor(1, "first", TEST_STACK_PEOPLE), None);
	let corrected = published_test_descriptor(2, "first", TEST_STACK_PEOPLE);

	let refused = serve(&mut source, &on_testnet(corrected.clone()), START);
	let record = serve(&mut source, &corrected, START + 3600);

	assert_eq!(
		(refused, record.is_some(), source.status()),
		(None, true, status(&corrected, Some(StackRefreshOutcome::Current)))
	);
}

/// A record may hold a copy on another TLD than the build bundles: one a core
/// stored before it held copies to the bundled TLD, or one from before an app
/// update moved the stack. The bundled copy starts the host, and the
/// published copy the record came from is refused when it is fetched again.
#[test]
fn a_stored_copy_on_another_tld_is_ignored() {
	let bundled = published_test_descriptor(1, "first", TEST_STACK_PEOPLE);
	let (link, _) = sign_test_descriptor(&bundled);
	let pointer = signed_pointer(&link).expect("test link carries a key");
	let moved = on_testnet(published_test_descriptor(2, "first", TEST_STACK_PEOPLE));
	let mut reader = StackReader::new(Refresh::Never);
	let record = reader
		.fetched(&pointer, Served::Document { bytes: &signed(&moved), etag: None }, START)
		.expect("the copy passes the descriptor rules")
		.stored
		.expect("a first copy is stored");

	let mut source = load(&bundled, Some(&record));
	let at_start = (source.stack().tld().to_string(), source.status());
	let fetched = serve(&mut source, &moved, START);

	assert_eq!(
		(at_start, fetched, source.status()),
		(
			("paseo".to_string(), status(&bundled, None)),
			None,
			status(&bundled, refused_for_its_tld())
		)
	);
}

/// The record a core holds once it stored `descriptor`.
fn record_holding(descriptor: &serde_json::Value) -> Vec<u8> {
	let (link, bytes) = sign_test_descriptor(descriptor);
	let pointer = signed_pointer(&link).expect("test link carries a key");
	StackReader::new(Refresh::Never)
		.fetched(&pointer, Served::Document { bytes: &bytes, etag: None }, START)
		.expect("the copy passes the descriptor rules")
		.stored
		.expect("a first copy is stored")
}

/// The reserved stack's copy at `sequence` with its dotNS contract at another
/// address.
fn redeployed(sequence: u32) -> serde_json::Value {
	let mut descriptor = reserved_test_descriptor(sequence);
	descriptor["dotns"]["popController"] = format!("0x{}", hex::encode([0xee; 20])).into();
	descriptor
}

/// A key that signs Polkadot's descriptor must not be able to point its
/// hosts at other dotNS contracts or other chains. The build's copy decides
/// both, so such a copy is refused on every fetch and never stored, and the
/// core keeps the copy it holds.
#[test]
fn a_reserved_stacks_refresh_that_changes_dotns_or_a_chain_is_refused_and_not_stored() {
	let mut moved_endpoint = reserved_test_descriptor(2);
	moved_endpoint["chains"][2]["rpc"] = serde_json::json!(["wss://moved.test.invalid"]);
	let mut wiped = reserved_test_descriptor(3);
	wiped["generation"] = "second".into();
	wiped["chains"][2]["genesisHash"] =
		format!("0x{}", hex::encode(TEST_STACK_WIPED_PEOPLE)).into();
	let refused_copies = [(redeployed(3), "dotns"), (wiped, "a chain's genesisHash")];

	assert_eq!(
		refused_copies.clone().map(|(copy, _)| {
			let mut source =
				load_reserving(&reserved_test_descriptor(1), None, ReservedValuesPolicy::ALLOWED);
			let held = serve(&mut source, &moved_endpoint, START);
			let refusals = [START + 3600, START + 7200]
				.map(|now| (serve(&mut source, &copy, now), source.status()));
			(held.is_some(), refusals)
		}),
		refused_copies.map(|(_, field)| {
			let refused = (None, status(&moved_endpoint, refused_for_pinned(field)));
			(true, [refused.clone(), refused])
		})
	);
}

/// polkadot-test is forked again from a later Polkadot block: the genesis
/// hashes stay and the `generation` changes. No pinned field differs, so the
/// stack follows it the way any stack follows a restart of its chains.
#[test]
fn a_reserved_stack_follows_a_new_generation_on_the_same_chains() {
	let bundled = reserved_test_descriptor(1);
	let mut reforked = reserved_test_descriptor(2);
	reforked["generation"] = "second".into();
	let mut source = load_reserving(&bundled, None, ReservedValuesPolicy::ALLOWED);

	let record = serve(&mut source, &reforked, START).expect("the newer copy is stored");
	source.record_stored();
	let restarted = load_reserving(&bundled, Some(&record), ReservedValuesPolicy::ALLOWED);

	assert_eq!(
		(source.status(), restarted.status()),
		(
			StackStatus {
				restart_required: true,
				..status(&bundled, Some(StackRefreshOutcome::Current))
			},
			status(&reforked, None)
		)
	);
}

/// A record may hold a copy that differs from the build's in a pinned field:
/// one a core stored before it pinned them, or one from before an app update
/// changed the field. The bundled copy starts the host, and the published
/// copy the record came from is refused when it is fetched again.
#[test]
fn a_reserved_stacks_stored_copy_that_changes_a_pinned_field_is_ignored() {
	let bundled = reserved_test_descriptor(1);
	let record = record_holding(&redeployed(2));

	let mut source = load_reserving(&bundled, Some(&record), ReservedValuesPolicy::ALLOWED);
	let at_start = source.status();
	let fetched = serve(&mut source, &redeployed(2), START);

	assert_eq!(
		(at_start, fetched, source.status()),
		(status(&bundled, None), None, status(&bundled, refused_for_pinned("dotns")))
	);
}

/// A stack without reserved values follows its operator: a redeployed
/// contract reaches its hosts at once, without a new build.
#[test]
fn a_stack_without_reserved_values_follows_a_redeployed_contract() {
	let on_paseo = |mut descriptor: serde_json::Value| {
		descriptor["tld"] = "paseo".into();
		descriptor
	};
	let redeployed = on_paseo(redeployed(2));
	let mut source = load(&on_paseo(reserved_test_descriptor(1)), None);

	let record = serve(&mut source, &redeployed, START);

	assert_eq!(
		(record.is_some(), source.status()),
		(true, status(&redeployed, Some(StackRefreshOutcome::Current)))
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
