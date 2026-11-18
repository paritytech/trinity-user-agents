//! The coinage calls and origins as the Android wallet encodes them. Each
//! vector is the output of the wallet's own builders, run from the fixed
//! inputs below against the People fixture's metadata (wallet commit
//! `190b0e581`), so the bytes this layer signs are the bytes the shipped
//! wallet signed. The wallet's native keys cannot run in its JVM tests, so
//! keys, signatures and proofs are fixed bytes.

use subxt::ext::{
	frame_decode::extrinsics::encode_call_data,
	scale_encode::{EncodeAsFields, EncodeAsType},
};

use super::super::*;
use crate::coinage::{chain, testing::people_metadata};

const SPLIT: &str = "44000801081111111111111111111111111111111111111111111111111111111111111111222222222222222222222222222222222222222222222222222222222222222202043333333333333333333333333333333333333333333333333333333333333333";
const TRANSFER: &str = "44014444444444444444444444444444444444444444444444444444444444444444";
const RECYCLE: &str = "4402555555555555555555555555555555555555555555555555555555555555555566666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666";
const SINGLE_LOAD: &str = "440f030000000003555555555555555555555555555555555555555555555555555555555555555566666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666666";
const UNLOAD_INTO_COINS: &str = "440d0300000008a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2040900000006000000080308111111111111111111111111111111111111111111111111111111111111111122222222222222222222222222222222222222222222222222222222222222220404333333333333333333333333333333333333333333333333333333333333333300000000000000000000000000000000";
const UNLOAD_INTO_EXTERNAL: &str = "44050300000008a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2040900000006000000888888888888888888888888888888888888888888888888888888888888888800000000000000000000000000000000";
const UNLOAD_AND_RELOAD: &str = "44090300000008a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a204090000000600000088888888888888888888888888888888888888888888888888888888888888881c0000000000000000000000000000000402bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb00000000000000000000000000000000";
const AS_COIN: &str = "0100";
const UNPAID: &str = "010505000000";
const TOKEN_PEOPLE: &str = "01011901c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c10b0000000c0000000403020107000000080ca7a7a70ca8a8a8";
const TOKEN_LITE_PEOPLE: &str = "01021901c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c1c10b0000000c0000000403020107000000080ca7a7a70ca8a8a8";
const RECYCLER_CONTEXT: &str = "706f703a706f6c6b61646f742e6e6574776f726b2f636f696e72656379636c72";
const FREE_TOKEN_CONTEXT: &str = "706f703a706f6c6b61646f742e6e65742f636f696e66746b0403020107000000";

fn call(name: &str, args: &impl EncodeAsFields) -> String {
	let metadata = people_metadata();
	hex::encode(encode_call_data("Coinage", name, args, &metadata, metadata.types()).unwrap())
}

fn origin(info: AsCoinageInfo) -> String {
	let metadata = people_metadata();
	let extension = metadata
		.extrinsic()
		.transaction_extensions_by_version(0)
		.unwrap()
		.find(|extension| extension.identifier() == chain::EXTENSION)
		.unwrap();
	hex::encode(Some(info).encode_as_type(extension.extra_ty(), metadata.types()).unwrap())
}

fn aliases() -> Vec<[u8; 32]> {
	vec![[0xa1; 32], [0xa2; 32]]
}

#[test]
fn a_split_encodes_as_the_wallet_does() {
	let args = SplitArgs { split_into: vec![SplitGroup(1, vec![[0x11; 32], [0x22; 32]]), SplitGroup(2, vec![[0x33; 32]])] };
	assert_eq!(call(chain::SPLIT, &args), SPLIT);
}

#[test]
fn a_transfer_encodes_as_the_wallet_does() {
	assert_eq!(call(chain::TRANSFER, &TransferArgs { to: [0x44; 32] }), TRANSFER);
}

#[test]
fn a_recycle_encodes_as_the_wallet_does() {
	let args = LoadWithCoinArgs { member_key: [0x55; 32], proof_of_ownership: [0x66; 64] };
	assert_eq!(call(chain::LOAD_WITH_COIN, &args), RECYCLE);
}

/// The wallet loads one entry per call (`SINGLE_LOAD`, for instance 3); this
/// layer batches them. Each item of the batch is the wallet's single load
/// after its pallet, call and instance, so the batch carries the same bytes
/// per entry.
#[test]
fn each_item_of_a_load_encodes_as_the_wallets_single_load() {
	let args = UnpaidBatchArgs {
		instance_id: 3,
		items: vec![UnpaidLoadInput {
			preservation: CodecPreservation::Expendable,
			value: 3,
			member_key: [0x55; 32],
			proof_of_ownership: [0x66; 64],
		}],
	};
	let item = &SINGLE_LOAD["440f03000000".len()..];
	assert_eq!(call(chain::LOAD_UNPAID_BATCH, &args), format!("44100300000004{item}"));
}

#[test]
fn an_unload_into_coins_encodes_as_the_wallet_does() {
	let args = UnloadToCoinsArgs {
		instance_id: 3,
		aliases: aliases(),
		value: 4,
		index: 9,
		revision: 6,
		split_into: vec![SplitGroup(3, vec![[0x11; 32], [0x22; 32]]), SplitGroup(4, vec![[0x33; 32]])],
		max_fee: 0,
	};
	assert_eq!(call(chain::UNLOAD_INTO_COINS, &args), UNLOAD_INTO_COINS);
}

#[test]
fn an_unload_into_the_external_asset_encodes_as_the_wallet_does() {
	let args =
		UnloadToExternalArgs { instance_id: 3, aliases: aliases(), value: 4, index: 9, revision: 6, to: [0x88; 32], max_fee: 0 };
	assert_eq!(call(chain::UNLOAD_INTO_EXTERNAL, &args), UNLOAD_INTO_EXTERNAL);
}

#[test]
fn an_unload_that_reloads_its_surplus_encodes_as_the_wallet_does() {
	let args = UnloadToExternalAndLoadedArgs {
		instance_id: 3,
		aliases: aliases(),
		value: 4,
		index: 9,
		revision: 6,
		to: [0x88; 32],
		external_asset_amount: 28,
		loaded_coins: vec![(2, [0xbb; 32])],
		max_fee: 0,
	};
	assert_eq!(call(chain::UNLOAD_INTO_EXTERNAL_AND_LOADED, &args), UNLOAD_AND_RELOAD);
}

#[test]
fn the_coin_and_unpaid_origins_encode_as_the_wallet_does() {
	assert_eq!([origin(AsCoinageInfo::AsCoin), origin(AsCoinageInfo::InfallibleUnpaidSigned { nonce: 5 })], [AS_COIN, UNPAID]);
}

#[test]
fn the_token_origins_encode_as_the_wallet_does() {
	let proof = || MembershipProof { proof: vec![0xc1; 70], ring: 11, revision: 12 };
	let alias_proofs = || vec![vec![0xa7; 3], vec![0xa8; 3]];
	let people =
		AsCoinageInfo::AsUnloadTokenPeople { proof: proof(), period: 0x0102_0304, counter: 7, alias_proofs: alias_proofs() };
	let lite_people =
		AsCoinageInfo::AsUnloadTokenLitePeople { proof: proof(), period: 0x0102_0304, counter: 7, alias_proofs: alias_proofs() };
	assert_eq!([origin(people), origin(lite_people)], [TOKEN_PEOPLE, TOKEN_LITE_PEOPLE]);
}

#[test]
fn the_proof_contexts_are_the_wallets() {
	let free_token = free_token_context(TokenId { period: 0x0102_0304, slot: 7 });
	assert_eq!([hex::encode(UNLOADING_RECYCLER_CONTEXT), hex::encode(free_token)], [RECYCLER_CONTEXT, FREE_TOKEN_CONTEXT]);
}
