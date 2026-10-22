//! A fee-less `Scarcity::transfer`, signed by the item key it moves from.

use parity_scale_codec::{Compact, Encode};
use subxt::{ArcMetadata, utils::H256};
use truapi::latest::TxPayloadExtension;

use crate::{
	chain::{EncodedExtrinsic, MortalExtrinsic, Mortality},
	host_internal::extrinsic::{LocalTransactionError, Sr25519Signer, build_signed_extrinsic_v5},
	nft::ItemKey,
};

/// The transaction extension pipeline that carries `AsScarcity`, signed by
/// `VerifyMultiSignature` rather than paid for.
const MOVE_PIPELINE_VERSION: u8 = 1;

/// One NFT move: the instance at the source key, the revision it must sign,
/// and the key it goes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NftMove {
	/// The instance the source key holds.
	pub instance: u64,
	/// The instance's `state_nonce` now, which the move signs over.
	pub state_nonce: u64,
	/// The key it goes to.
	pub destination: ItemKey,
}

/// The runtime a move is signed for, as of the block its era is born at.
#[derive(Clone, Debug)]
pub struct MoveRuntime {
	/// Genesis of the chain.
	pub genesis: H256,
	/// Spec version at the birth block.
	pub spec_version: u32,
	/// Transaction version at the birth block.
	pub transaction_version: u32,
	/// Metadata at the birth block.
	pub metadata: ArcMetadata,
}

/// `nft_move` signed by `signer`, valid during `mortality`.
pub fn sign_move(
	signer: &Sr25519Signer,
	runtime: &MoveRuntime,
	nft_move: &NftMove,
	mortality: Mortality,
) -> Result<MortalExtrinsic, LocalTransactionError> {
	let call = transfer_call(&runtime.metadata, nft_move.destination)?;
	let extensions = move_extensions(runtime, nft_move, mortality);
	let genesis = runtime.genesis.0;
	let metadata = runtime.metadata.clone();
	let bytes = build_signed_extrinsic_v5(
		signer,
		genesis,
		&call,
		&extensions,
		MOVE_PIPELINE_VERSION,
		metadata,
	)?;
	Ok(MortalExtrinsic { extrinsic: EncodedExtrinsic::new(bytes), mortality })
}

/// `Scarcity::transfer { to: destination }`, with the indices the runtime
/// gives the pallet and the call.
fn transfer_call(
	metadata: &ArcMetadata,
	destination: ItemKey,
) -> Result<Vec<u8>, LocalTransactionError> {
	let missing = || LocalTransactionError::Other("runtime has no Scarcity::transfer".to_string());
	let pallet = metadata.pallet_by_name("Scarcity").ok_or_else(missing)?;
	let call = pallet.call_variant_by_name("transfer").ok_or_else(missing)?;
	Ok([&[pallet.call_index(), call.index][..], &destination.0].concat())
}

/// Every extension of the move pipeline, by name. Only `AsScarcity` names the
/// NFT; the rest say the move pays no fee and needs no account nonce.
fn move_extensions(
	runtime: &MoveRuntime,
	nft_move: &NftMove,
	mortality: Mortality,
) -> Vec<TxPayloadExtension> {
	let birth = mortality.birth().hash.0.to_vec();
	vec![
		extension("UnitTransactionExtension", Vec::new(), Vec::new()),
		extension("AsScarcity", as_scarcity(nft_move), Vec::new()),
		extension("AuthorizeCall", Vec::new(), Vec::new()),
		extension("AsPgas", vec![0], Vec::new()),
		extension("AsDotnsGateway", vec![0], Vec::new()),
		extension("RestrictOrigins", vec![0], Vec::new()),
		extension("CheckNonZeroSender", Vec::new(), Vec::new()),
		extension("CheckSpecVersion", Vec::new(), runtime.spec_version.encode()),
		extension("CheckTxVersion", Vec::new(), runtime.transaction_version.encode()),
		extension("CheckGenesis", Vec::new(), runtime.genesis.0.to_vec()),
		extension("CheckMortality", mortality.era_bytes().to_vec(), birth),
		extension("CheckNonce", Compact(0_u32).encode(), Vec::new()),
		extension("CheckWeight", Vec::new(), Vec::new()),
		extension("ChargeAssetTxPayment", vec![0, 0], Vec::new()),
		extension("PrevalidateAttests", Vec::new(), Vec::new()),
		extension("CheckMetadataHash", vec![0], vec![0]),
		extension("EthSetOrigin", Vec::new(), Vec::new()),
		extension("StorageWeightReclaim", Vec::new(), Vec::new()),
	]
}

/// The pallet's `AsScarcityInfo`: which NFT a transaction acts as.
#[derive(Encode)]
enum AsScarcityInfo {
	AsNft { instance: u64, state_nonce: u64 },
}

/// `AsScarcity`'s value for a move: `Some(AsNft { instance, state_nonce })`.
fn as_scarcity(nft_move: &NftMove) -> Vec<u8> {
	let NftMove { instance, state_nonce, .. } = *nft_move;
	Some(AsScarcityInfo::AsNft { instance, state_nonce }).encode()
}

fn extension(id: &str, extra: Vec<u8>, additional_signed: Vec<u8>) -> TxPayloadExtension {
	TxPayloadExtension { id: id.to_string(), extra, additional_signed }
}

#[cfg(test)]
mod tests;
