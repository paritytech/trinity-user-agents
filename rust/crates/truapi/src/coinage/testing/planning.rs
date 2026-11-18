//! Fixtures for the selection, classification, balance and maintenance tests
//! ported from the shipped wallet: chain reads with chosen ages and ring
//! counts, observation at a chosen time, presented tokens, and parameters
//! with a smaller unload cap.

use futures::executor::block_on;
use parity_scale_codec::{Decode, Encode};
use subxt::Metadata;

use super::{PEOPLE_METADATA_BYTES, chain_config, instance_record, register_rows, unbuilt_engine};
use crate::{
	coinage::{
		Asset, ChainParams, Exponent, PurseId,
		chain::{MAX_CONSOLIDATION, PALLET},
		ledger::{AssetKind, BlockRef, CoinOnChain, EntryOnChain, Facts, Read, Subject, TokenId, allocate, observe},
		operation::OpKey,
		params::validate,
		registration::{Admission, Rows},
		tokens::period_of,
	},
	store::Db,
};

/// A fresh asset of `kind` and `exponent` in `purse`, not yet observed.
pub fn allocated(db: &Db, purse: PurseId, kind: AssetKind, exponent: u8) -> Asset {
	let outputs = [Exponent::new(exponent).expect("a legal exponent")];
	block_on(db.write(move |tx| allocate(tx, purse, kind, &outputs))).expect("allocated")[0]
}

/// Records `read` of `asset` as F1 would at `now`; an entry's key first seen
/// included starts its dwell at `now`.
pub fn observe_at(db: &Db, asset: Asset, read: Read, now: u64) {
	let facts = Facts { at: BlockRef { number: 1, hash: [1; 32] }, reads: vec![(Subject::Asset(asset), read)] };
	block_on(db.write(move |tx| observe(tx, &facts, now, 255))).expect("observed");
}

/// An unlocked coin of `age` as F1 reads it.
pub fn aged_coin(exponent: u8, age: u32) -> Read {
	Read::Coin {
		now: Some(CoinOnChain { exponent: Exponent::new(exponent).expect("a legal exponent"), age, locked_until: None }),
		emptied_by: None,
	}
}

/// An entry in an open `ring` whose root includes all of its `members`,
/// the entry's own key among them.
pub fn in_ring(ring: u32, members: u32) -> Read {
	Read::Entry(Some(EntryOnChain {
		ring,
		revision: 1,
		page: 0,
		position: 0,
		members,
		included: members,
		closed: false,
		expires_at: None,
		locked_until: None,
		unloaded: None,
	}))
}

/// Registers one record per slot presenting the first `count` tokens of the
/// period that contains `now`, so they are held.
pub fn present_tokens(db: &Db, count: u32, now: u64, params: &ChainParams) {
	let engine = unbuilt_engine(db.clone());
	let period = period_of(now, params);
	for slot in 0..count {
		let mut id = [0xee; 32];
		id[..4].copy_from_slice(&slot.to_le_bytes());
		let operation = OpKey { scope: b"tokens".to_vec(), id };
		let rows = Rows { tokens: vec![TokenId { period, slot }], ..Rows::default() };
		register_rows(&engine, &operation, Admission::Ordinary, vec![rows]).expect("the token is free");
	}
}

/// Parameters validated as [`super::chain_params`], from a People runtime
/// whose unload cap is `cap` entries.
pub fn params_with_unload_cap(cap: u32) -> ChainParams {
	let mut prefixed = frame_metadata::RuntimeMetadataPrefixed::decode(&mut &PEOPLE_METADATA_BYTES[..])
		.expect("the People fixture decodes");
	let frame_metadata::RuntimeMetadata::V16(runtime) = &mut prefixed.1 else {
		panic!("the People fixture is metadata V16");
	};
	let pallet = runtime.pallets.iter_mut().find(|pallet| pallet.name == PALLET).expect("the coinage pallet");
	let constant =
		pallet.constants.iter_mut().find(|constant| constant.name == MAX_CONSOLIDATION).expect("the unload cap");
	constant.value = cap.encode();
	let metadata = Metadata::try_from(prefixed).expect("still subxt metadata");
	validate(&metadata, Some(&instance_record()), Some(16), chain_config()).expect("the runtime serves the layer")
}
