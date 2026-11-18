use super::*;
use crate::test_support::{TEST_STACK_PEOPLE, TEST_STACK_WIPED_PEOPLE, published_test_descriptor};

fn descriptor(json: &serde_json::Value) -> Descriptor {
	serde_json::from_value(json.clone()).expect("test descriptor is valid")
}

/// Accounts derive under the TLD, so a copy on another one would change every
/// account the host holds. Everything else may differ: a stack follows a wipe
/// without a new build.
#[test]
fn a_copy_passes_only_on_the_bundled_tld() {
	let bundled = published_test_descriptor(1, "first", TEST_STACK_PEOPLE);
	let wiped = published_test_descriptor(2, "second", TEST_STACK_WIPED_PEOPLE);
	let mut on_another_tld = wiped.clone();
	on_another_tld["tld"] = "testnet".into();
	let pinned = Pinned::new(&descriptor(&bundled));

	assert_eq!(
		[&bundled, &wiped, &on_another_tld].map(|copy| pinned.refuse_change(&descriptor(copy))),
		[Ok(()), Ok(()), Err(StackError::PinnedFieldChanged { field: "tld" })]
	);
}
