use schnorrkel::{ExpansionMode, MiniSecretKey};
use subxt::ext::frame_decode::extrinsics::decode_extrinsic;

use super::*;
use crate::{chain::HashAndNumber, nft::scarcity::testing::asset_hub_metadata};

const DESTINATION: ItemKey = ItemKey([0x08; 32]);
const BIRTH: HashAndNumber = HashAndNumber { hash: H256([0x0b; 32]), number: 2_138_796 };

fn test_signer() -> Sr25519Signer {
	let keypair = MiniSecretKey::from_bytes(&[7; 32])
		.unwrap()
		.expand_to_keypair(ExpansionMode::Ed25519);
	Sr25519Signer::from_keypair(&keypair)
}

fn asset_hub_runtime() -> TransferRuntime {
	TransferRuntime {
		genesis: H256([0xab; 32]),
		spec_version: 3_004_000,
		transaction_version: 22,
		metadata: asset_hub_metadata(),
	}
}

fn test_transfer() -> NftTransfer {
	NftTransfer { instance: 330, state_nonce: 7, destination: DESTINATION }
}

#[test]
fn a_move_is_a_signed_transfer_to_the_destination() {
	let signer = test_signer();
	let mortality = Mortality::new(BIRTH, 64).unwrap();
	let runtime = asset_hub_runtime();

	let signed = sign_transfer(&signer, &runtime, &test_transfer(), mortality).unwrap();

	let bytes = signed.extrinsic.bytes();
	let decoded =
		decode_extrinsic(&mut &bytes[..], &*runtime.metadata, runtime.metadata.types()).unwrap();
	let call_args = &bytes[decoded.call_data_args_range()];
	let pallet_and_call = (decoded.pallet_name(), decoded.call_name());
	assert_eq!(pallet_and_call, ("Scarcity", "transfer"));
	assert_eq!(call_args, DESTINATION.0);
	assert_eq!(signed.mortality, mortality);
}

#[test]
fn a_move_acts_as_the_nft_it_names_without_fee_or_nonce() {
	let signer = test_signer();
	let mortality = Mortality::new(BIRTH, 64).unwrap();
	let runtime = asset_hub_runtime();

	let signed = sign_transfer(&signer, &runtime, &test_transfer(), mortality).unwrap();

	let bytes = signed.extrinsic.bytes();
	let decoded =
		decode_extrinsic(&mut &bytes[..], &*runtime.metadata, runtime.metadata.types()).unwrap();
	let extensions = decoded.transaction_extension_payload().unwrap();
	let value_of = |name: &str| {
		let extension = extensions.iter().find(|extension| extension.name() == name).unwrap();
		bytes[extension.range()].to_vec()
	};
	let as_scarcity = [&[1_u8, 0][..], &330_u64.to_le_bytes(), &7_u64.to_le_bytes()].concat();
	assert_eq!(value_of("AsScarcity"), as_scarcity);
	assert_eq!(value_of("CheckMortality"), mortality.era_bytes());
	assert_eq!(value_of("CheckNonce"), [0]);
	assert_eq!(value_of("ChargeAssetTxPayment"), [0, 0]);
}
