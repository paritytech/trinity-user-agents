use futures::executor::block_on;
use schnorrkel::{ExpansionMode, MiniSecretKey};
use subxt::ext::frame_decode::extrinsics::decode_extrinsic;

use super::*;
use crate::{
	chain::{DEFAULT_MORTALITY_PERIOD, extrinsic_service_serving},
	nft::scarcity::testing::{ASSET_HUB_METADATA_BYTES, ASSET_HUB_PIPELINE, asset_hub_metadata},
};

const DESTINATION: ItemKey = ItemKey([0x08; 32]);
/// The genesis the scripted node's chain is configured with.
const GENESIS: H256 = H256([0xab; 32]);

fn test_signer() -> Sr25519Signer {
	let keypair = MiniSecretKey::from_bytes(&[7; 32])
		.unwrap()
		.expand_to_keypair(ExpansionMode::Ed25519);
	Sr25519Signer::from_keypair(&keypair)
}

fn test_transfer() -> NftTransfer {
	NftTransfer { instance: 330, state_nonce: 7, destination: DESTINATION }
}

/// [`test_transfer`] built over a scripted node serving the Asset Hub fixture.
fn signed_test_transfer() -> MortalExtrinsic {
	let extrinsics = extrinsic_service_serving(ASSET_HUB_METADATA_BYTES, ASSET_HUB_PIPELINE);
	block_on(extrinsics.sign_nft_transfer(GENESIS, &test_signer(), &test_transfer())).unwrap()
}

#[test]
fn a_transfer_is_a_signed_scarcity_transfer_to_the_destination() {
	let metadata = asset_hub_metadata();

	let signed = signed_test_transfer();

	let bytes = signed.extrinsic.bytes();
	let decoded = decode_extrinsic(&mut &bytes[..], &*metadata, metadata.types()).unwrap();
	let call_args = &bytes[decoded.call_data_args_range()];
	let pallet_and_call = (decoded.pallet_name(), decoded.call_name());
	assert_eq!(pallet_and_call, ("Scarcity", "transfer"));
	assert_eq!(call_args, DESTINATION.0);
	assert_eq!(signed.mortality.period(), DEFAULT_MORTALITY_PERIOD);
}

#[test]
fn a_transfer_acts_as_the_nft_it_names_without_fee_or_nonce() {
	let metadata = asset_hub_metadata();

	let signed = signed_test_transfer();

	let bytes = signed.extrinsic.bytes();
	let decoded = decode_extrinsic(&mut &bytes[..], &*metadata, metadata.types()).unwrap();
	let extensions = decoded.transaction_extension_payload().unwrap();
	let value_of = |name: &str| {
		let extension = extensions.iter().find(|extension| extension.name() == name).unwrap();
		bytes[extension.range()].to_vec()
	};
	let as_scarcity = [&[1_u8, 0][..], &330_u64.to_le_bytes(), &7_u64.to_le_bytes()].concat();
	assert_eq!(value_of("AsScarcity"), as_scarcity);
	assert_eq!(value_of("CheckNonce"), [0]);
	assert_eq!(value_of("ChargeAssetTxPayment"), [0, 0]);
}

/// The pallet's `InvalidTransaction::Custom` code for a source key that holds
/// no NFT.
const NO_NFT: u8 = 2;

/// The node decodes a signed transfer and checks its signature before it
/// looks for the NFT, so a transfer from a key that holds nothing failing
/// with the pallet's "no NFT" code proves the encoding and the signature.
/// Run with `cargo test -p truapi --lib nft::scarcity::transfer -- --ignored`.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a live Asset Hub"]
async fn the_live_node_refuses_a_transfer_from_an_empty_key_as_holding_no_nft() {
	use subxt::tx::{TransactionInvalid, ValidationResult};

	use crate::chain::{
		FixedExtensionVersions, TxValidator, live_asset_hub, live_asset_hub_genesis,
	};

	let genesis = live_asset_hub_genesis();
	let chain = live_asset_hub(genesis);
	let host = FixedExtensionVersions::one_chain(genesis, ASSET_HUB_PIPELINE);
	let extrinsics = ExtrinsicService::over_chain(chain.clone(), std::sync::Arc::new(host));

	let signed = extrinsics
		.sign_nft_transfer(genesis, &test_signer(), &test_transfer())
		.await
		.unwrap();
	let validated = chain.validate(genesis, &signed.extrinsic).await.expect("node validates");

	let no_nft = ValidationResult::Invalid(TransactionInvalid::Custom(NO_NFT));
	assert_eq!(format!("{validated:?}"), format!("{no_nft:?}"));
}
