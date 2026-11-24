//! Scripted chain answers for slot scans.

use crate::runtime::statement_allowance::{
	CollectionCandidate, FixedPersonhoodSigner, collection::PersonhoodCollection,
};

use super::{slot_alias, statement_store_allowance_key};

/// The only membership a test signer holds: `entropy` in LitePeople.
pub fn lite_people(entropy: [u8; 32]) -> [CollectionCandidate; 1] {
	[CollectionCandidate { collection: PersonhoodCollection::LitePeople, entropy }]
}

/// A `state_queryStorageAt` answer for `entropy`'s `period` row, where
/// `entries[seq]` is slot `seq`'s JSON storage value. Like the node, it
/// lists only the slots that exist.
pub fn slot_row(
	entropy: [u8; 32],
	network_suffix: &[u8],
	period: u32,
	entries: &[Option<String>],
) -> String {
	let candidates = lite_people(entropy);
	let signer = futures::executor::block_on(FixedPersonhoodSigner::new(&candidates)).unwrap();
	let changes: Vec<String> = entries
		.iter()
		.enumerate()
		.filter_map(|(seq, entry)| {
			let alias = slot_alias(
				&signer,
				PersonhoodCollection::LitePeople,
				network_suffix,
				period,
				u32::try_from(seq).expect("slot sequence fits in u32"),
			)
			.unwrap();
			let key = hex::encode(statement_store_allowance_key(period, &alias));
			Some(format!(r#"["0x{key}",{}]"#, entry.as_ref()?))
		})
		.collect();
	format!(r#"[{{"block":"0xb10c","changes":[{}]}}]"#, changes.join(","))
}
