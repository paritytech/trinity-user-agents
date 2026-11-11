use subxt_rpcs::RpcClient as HostRpcClient;

use super::{
	super::{rpc::testing::ScriptedRpc, test_fixtures},
	*,
};

/// The collection the captured roots were read from. The fixture only means
/// anything paired with this identifier.
const CAPTURED_COLLECTION: PersonhoodCollection = PersonhoodCollection::LitePeople;
const TEST_GENERATION: u32 = 7;

/// The captured ring-5 roots as a scripted `state_getStorage` result, with the
/// transport handle so the key that was read can be checked.
fn scripted_ring_5_roots() -> (RpcClient, ScriptedRpc) {
	let value = format!(r#""0x{}""#, hex::encode(test_fixtures::ASSET_HUB_RING_5_ROOTS));
	let generation = format!(r#""0x{}""#, hex::encode(TEST_GENERATION.to_le_bytes()));
	let scripted = ScriptedRpc::new([generation.as_str(), value.as_str()]);
	(RpcClient::new(HostRpcClient::new(scripted.clone())), scripted)
}

/// `ScriptedRpc` replays by position and ignores the key, so without this the
/// collection paired with the captured blob would be a comment rather than a
/// fact: re-capturing the blob against another collection, or editing the
/// constant, would leave every test in this module green.
///
/// The expected key is built from the identifier literal the fixtures README
/// records the capture under, not from `CAPTURED_COLLECTION`. Deriving both
/// sides from the constant would move them together and assert nothing.
fn assert_read_ring_5_of(scripted: &ScriptedRpc) {
	const CAPTURED_UNDER: &[u8; 32] = b"pop:polkadot.network/people-lite";
	assert_eq!(
		CAPTURED_COLLECTION.identifier(),
		CAPTURED_UNDER,
		"the committed blob was read under the lite-people identifier",
	);
	let current_generation = format!(
		r#"["0x{}"]"#,
		hex::encode(
			[twox_128(b"MembersSubscriber").as_slice(), twox_128(b"CurrentGeneration").as_slice(),]
				.concat()
		)
	);
	let generation = twox_64_concat(&TEST_GENERATION.to_le_bytes());
	let ring_roots = format!(
		r#"["0x{}"]"#,
		hex::encode(
			[
				twox_128(b"MembersSubscriber").as_slice(),
				twox_128(b"RingRoots").as_slice(),
				&generation,
				&blake2_128_concat(CAPTURED_UNDER),
				&blake2_128_concat(&5u32.to_le_bytes()),
			]
			.concat()
		)
	);
	let reads: Vec<String> = scripted
		.calls()
		.into_iter()
		.filter(|(method, _)| method == "state_getStorage")
		.map(|(_, params)| params)
		.collect();
	assert_eq!(
		reads,
		vec![current_generation, ring_roots],
		"the roots read must use the current generation and the fixture collection"
	);
}

/// `RingCommitmentRecord` takes one field out of a much wider runtime struct,
/// so decode a captured value rather than a hand-built one: the projection is
/// the part that a changed record layout would silently break.
#[test]
fn captured_ring_roots_project_to_their_revisions() {
	let metadata = test_fixtures::asset_hub();
	let value_type = metadata.storage_value_type("MembersSubscriber", "RingRoots").expect(
		"the metadata fixture declares MembersSubscriber; \
             re-capture it and the roots blob together",
	);

	let records = Vec::<RingCommitmentRecord>::decode_as_type(
		&mut &test_fixtures::ASSET_HUB_RING_5_ROOTS[..],
		value_type,
		metadata.registry(),
	)
	.unwrap();

	assert_eq!(
		records.iter().map(|record| record.revision).collect::<Vec<_>>(),
		vec![105, 106, 108],
	);
}

/// End to end over the real path, against roots Asset Hub actually served: a
/// revision the window skipped reports as pruned instead of being waited out.
///
/// Only the two terminating outcomes are reachable here. Pending keeps
/// polling until `RING_REVISION_WAIT` expires, so it is covered by the
/// `revision_status` table instead.
#[test]
fn a_skipped_revision_reports_as_pruned_against_captured_roots() {
	let (rpc, scripted) = scripted_ring_5_roots();
	let err = futures::executor::block_on(await_ring_revision(
		&rpc,
		test_fixtures::asset_hub(),
		CAPTURED_COLLECTION,
		5,
		107,
	))
	.expect_err("revision 107 is missing from the captured window");
	assert_read_ring_5_of(&scripted);

	assert!(
		err.to_string().contains("pruned"),
		"a skipped revision should not be waited out: {err}"
	);
}

/// And one the window holds returns without a second poll.
#[test]
fn a_held_revision_returns_against_captured_roots() {
	let (rpc, scripted) = scripted_ring_5_roots();
	futures::executor::block_on(await_ring_revision(
		&rpc,
		test_fixtures::asset_hub(),
		CAPTURED_COLLECTION,
		5,
		106,
	))
	.expect("revision 106 is in the captured window");
	assert_read_ring_5_of(&scripted);
}

/// A wrong key hasher makes imported roots look absent until timeout.
#[test]
fn subscriber_ring_key_matches_the_runtime_layout() {
	assert_eq!(
		ring_roots_key(7, PersonhoodCollection::LitePeople, 136),
		hex::decode(
			"c8d053ab324196afc756c5ae3fbd29178034a4ffd558299b9ac8097424772405\
             0e0d969b0e48cab7070000000bf6e234fafa085fd6710b952081ed8d706f703a\
             706f6c6b61646f742e6e6574776f726b2f70656f706c652d6c697465751c83\
             d291ed9a90887239b8a92828a688000000"
		)
		.unwrap(),
	);
}

#[test]
fn missing_current_generation_uses_the_runtime_default() {
	let scripted = ScriptedRpc::new(["null"]);
	let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));

	let generation =
		futures::executor::block_on(read_current_generation(&rpc, test_fixtures::asset_hub()))
			.unwrap();

	assert_eq!(
		(generation, scripted.calls()),
		(
			0,
			vec![(
				"state_getStorage".to_string(),
				r#"["0xc8d053ab324196afc756c5ae3fbd2917c2dbc4fc2f665a39ada06f0965cccf86"]"#
					.to_string(),
			)],
		),
	);
}

/// Caching the generation would leave later polls on abandoned roots.
#[test]
fn a_generation_change_during_wait_moves_the_ring_root_read() {
	let first_generation = format!(r#""0x{}""#, hex::encode(7u32.to_le_bytes()));
	let second_generation = format!(r#""0x{}""#, hex::encode(8u32.to_le_bytes()));
	let roots = format!(r#""0x{}""#, hex::encode(test_fixtures::ASSET_HUB_RING_5_ROOTS));
	let scripted = ScriptedRpc::new([
		first_generation.as_str(),
		"null",
		second_generation.as_str(),
		roots.as_str(),
	]);
	let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));

	futures::executor::block_on(await_ring_revision(
		&rpc,
		test_fixtures::asset_hub(),
		CAPTURED_COLLECTION,
		5,
		106,
	))
	.unwrap();

	let storage_call =
		|key: Vec<u8>| ("state_getStorage".to_string(), format!(r#"["0x{}"]"#, hex::encode(key)));
	assert_eq!(
		scripted.calls(),
		vec![
			storage_call(current_generation_key()),
			storage_call(ring_roots_key(7, CAPTURED_COLLECTION, 5)),
			storage_call(current_generation_key()),
			storage_call(ring_roots_key(8, CAPTURED_COLLECTION, 5)),
		],
	);
}

/// A trailing byte means the entry is no longer a bare `u32`, which is the
/// same layout drift the three-key ring-root key exists to track. Taking the
/// first four bytes would build keys for a generation nothing answers, and
/// the wait would read as "the ring never arrived".
#[test]
fn a_current_generation_that_is_not_a_bare_u32_is_rejected() {
	let overlong = format!(r#""0x{}""#, hex::encode([7u8, 0, 0, 0, 0]));
	let scripted = ScriptedRpc::new([overlong.as_str()]);
	let rpc = RpcClient::new(HostRpcClient::new(scripted));

	let err =
		futures::executor::block_on(read_current_generation(&rpc, test_fixtures::asset_hub()))
			.expect_err("a five-byte value is not a u32");

	assert_eq!(
		err.to_string(),
		"MembersSubscriber.CurrentGeneration: Input buffer has still data left after decoding!"
	);
}

/// An absent value is only the `ValueQuery` default if the runtime declares
/// the entry at all. A renamed pallet or item reads as absent too, and
/// defaulting there would key every ring-root read at generation 0.
#[test]
fn a_runtime_without_current_generation_is_named_rather_than_defaulted() {
	// Real metadata, because the regression depends on the pallet being absent.
	let people = test_fixtures::people();
	let scripted = ScriptedRpc::new(["null"]);
	let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));

	let err = futures::executor::block_on(read_current_generation(&rpc, people))
		.expect_err("the People runtime declares no MembersSubscriber");

	assert_eq!(
		(err.to_string(), scripted.calls()),
		("MembersSubscriber.CurrentGeneration type not in metadata".to_string(), vec![],),
		"the metadata check comes before the read, so nothing is asked of the chain",
	);
}

/// `holds_a_full_claim` reads both of these from the runtime, and compares a
/// balance against the claim amount. A missing constant would make the warm
/// check answer the same way for every account.
#[test]
fn the_asset_hub_fixture_declares_the_pgas_asset_and_claim_amount() {
	let metadata = test_fixtures::asset_hub();

	assert_eq!(metadata.constant_u32("Pgas", "PgasAssetId").unwrap(), 2_000_000_000);
	assert_eq!(metadata.constant_u128("Pgas", "PgasClaimAmount").unwrap(), 50_000_000_000,);
}

#[test]
fn claim_completion_waits_for_the_full_credited_balance() {
	let partial = format!(r#""0x{}""#, hex::encode(49_999_999_999u128.to_le_bytes()));
	let full = format!(r#""0x{}""#, hex::encode(50_000_000_000u128.to_le_bytes()));
	let scripted = ScriptedRpc::new(["null", partial.as_str(), full.as_str()]);
	let rpc = RpcClient::new(HostRpcClient::new(scripted.clone()));
	futures::executor::block_on(async {
		let completion = await_full_claim(&rpc, test_fixtures::asset_hub(), &[0x22; 32], "0xb10c");
		futures::pin_mut!(completion);
		assert!(futures::poll!(&mut completion).is_pending());
		completion.await.expect("the fully credited claim becomes usable");
	});
	assert_eq!(
		scripted
			.calls()
			.iter()
			.filter(|(method, _)| method == "state_getStorage")
			.count(),
		3,
		"neither an absent account nor a partial credit can complete the claim"
	);
}

/// The window holds it, so the proof can be verified.
#[test]
fn a_held_revision_is_imported() {
	assert_eq!(revision_status(&[105, 106, 108], 106), RevisionStatus::Imported);
	assert_eq!(revision_status(&[106], 106), RevisionStatus::Imported);
}

/// Nothing newer is held, so ours may still be on its way. An empty window
/// reaches this if the entry exists but holds no roots; an absent entry
/// skips the classification entirely and goes straight to the next poll.
#[test]
fn a_revision_newer_than_the_window_is_pending() {
	assert_eq!(revision_status(&[], 106), RevisionStatus::Pending);
	assert_eq!(revision_status(&[103, 104, 105], 106), RevisionStatus::Pending);
}

/// A revision the window skipped is as unreachable as one that fell off the
/// front. Testing the oldest held root instead would wait this one out.
#[test]
fn a_skipped_revision_is_pruned_rather_than_pending() {
	assert_eq!(revision_status(&[105, 106, 108], 107), RevisionStatus::Pruned);
}

/// Evicted off the front of the window.
#[test]
fn a_revision_older_than_the_window_is_pruned() {
	assert_eq!(revision_status(&[105, 106, 108], 42), RevisionStatus::Pruned);
}

/// Storage does not promise an order, so the rule cannot depend on one.
#[test]
fn the_held_order_does_not_change_the_answer() {
	assert_eq!(revision_status(&[108, 105, 106], 107), RevisionStatus::Pruned);
	assert_eq!(revision_status(&[108, 105, 106], 106), RevisionStatus::Imported);
}

/// A claim is authorized against one revision, so the day it is built for and
/// the revision it proves must both reach the runtime unchanged.
#[test]
fn a_pruned_revision_is_distinguished_from_one_still_arriving() {
	let pruned = PgasError::RingRevisionPruned { ring_index: 2, revision: 100 };
	let waiting = PgasError::RingRevisionTimeout { ring_index: 2, revision: 100 };

	assert!(pruned.to_string().contains("pruned"));
	assert!(waiting.to_string().contains("has not imported"));
}

#[test]
fn asset_account_key_matches_asset_hub_vector() {
	let account: [u8; 32] = core::array::from_fn(|index| (index + 1) as u8);
	assert_eq!(
		hex::encode(pgas_balance_key(2_000_000_000, &account)),
		"682a59d51ab9e48a8c8cc418ff9708d2b99d880ec681799c0cf30e8886371da946de6e688c2258c9eb90a9b5c834b13d009435772dccd599abfe1920a1cff8a7358231430102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20"
	);
}

#[test]
fn asset_account_balance_accepts_trailing_fields() {
	let expected = u128::MAX - 7;
	let mut value = expected.to_le_bytes().to_vec();
	value.extend_from_slice(&[0xaa, 0xbb]);
	assert_eq!(decode_asset_account_balance(&value).unwrap(), expected);
}
