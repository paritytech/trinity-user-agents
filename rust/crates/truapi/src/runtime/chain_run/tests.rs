use super::*;
use crate::test_support::{MemoryCoreStorage, paseo_test_stack, test_stack, test_storage};

/// The same stack after a wipe whose operator left `generation` as it was:
/// only the genesis hashes differ.
fn wiped_stack() -> Stack {
	test_stack("paseo", [0x77; 32], [0xbb; 32], Some([0xcc; 32]))
}

/// A wipe has to be noticed once, and only once: the data read from the old
/// chains is dropped, and what the session then stores about the new chains
/// is kept on the next start.
#[test]
fn a_recorded_run_is_current_until_the_chains_restart() {
	let host = MemoryCoreStorage::default();
	let storage = test_storage(&host);

	futures::executor::block_on(async {
		let first_start = ChainRun::default();
		let before_recording = first_start.stored(&storage, &paseo_test_stack()).await;
		first_start.record(&storage, &paseo_test_stack()).await;
		let after_recording = first_start.stored(&storage, &paseo_test_stack()).await;

		let same_chains = ChainRun::default().stored(&storage, &paseo_test_stack()).await;
		let wiped = ChainRun::default().stored(&storage, &wiped_stack()).await;

		assert_eq!(
			[before_recording, after_recording, same_chains, wiped],
			[
				Some(StoredRun::Unrecorded),
				Some(StoredRun::Current),
				Some(StoredRun::Current),
				Some(StoredRun::Earlier),
			]
		);
	});
}

/// A host that cannot be read says nothing about the run. Taking that for
/// "nothing recorded" would record the current run over an earlier one, and
/// the data read from the old chains would never be dropped.
#[test]
fn a_run_that_cannot_be_read_or_recorded_stays_unknown() {
	let host = crate::test_support::StubPlatform {
		local_storage_error: Some("storage is down"),
		..Default::default()
	};
	let storage = test_storage(&host);
	let run = ChainRun::default();

	futures::executor::block_on(async {
		let unread = run.stored(&storage, &paseo_test_stack()).await;
		run.record(&storage, &paseo_test_stack()).await;
		let unrecorded = run.stored(&storage, &paseo_test_stack()).await;

		assert_eq!((unread, unrecorded), (None, None));
	});
}
