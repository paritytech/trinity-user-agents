//! A fee-less `Scarcity::transfer`, signed by the item key it transfers from.

use subxt::{ext::scale_encode::EncodeAsType, utils::H256};

use crate::{
	chain::{
		ExtensionError, ExtrinsicBuildError, ExtrinsicService, ExtrinsicVersion,
		InheritedImplication, MortalExtrinsic, TransactionExtension,
	},
	host_internal::extrinsic::Sr25519Signer,
	nft::ItemKey,
};

/// The transaction extension pipeline that carries `AsScarcity`, signed by
/// `VerifyMultiSignature` rather than paid for.
const TRANSFER_PIPELINE_VERSION: u8 = 1;

/// Blocks a transfer stays valid for after its birth.
pub const TRANSFER_PERIOD: u64 = 64;

/// One NFT transfer: the instance at the source key, the revision it must sign,
/// and the key it goes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NftTransfer {
	/// The instance the source key holds.
	pub instance: u64,
	/// The instance's `state_nonce` now, which the transfer signs over.
	pub state_nonce: u64,
	/// The key it goes to.
	pub destination: ItemKey,
}

/// `nft_transfer` on the chain `genesis`, signed by `signer` and built by
/// `extrinsics`, which decides the block it is born at. It pays no fee and
/// uses no account nonce.
pub async fn sign_transfer(
	extrinsics: &ExtrinsicService,
	genesis: H256,
	signer: &Sr25519Signer,
	nft_transfer: &NftTransfer,
) -> Result<MortalExtrinsic, ExtrinsicBuildError> {
	let call = TransferCallArgs { to: nft_transfer.destination.0 };
	let version = ExtrinsicVersion::V5 { extension_version: TRANSFER_PIPELINE_VERSION };
	extrinsics
		.build_mortal_extrinsic(genesis)
		.with_call_args("Scarcity", "transfer", call)
		.with_extension(AsScarcityExtension::from(nft_transfer))
		.with_signer(signer)
		.with_nonce(0)
		.with_mortality_period(TRANSFER_PERIOD)
		.with_extrinsic_version(version)
		.build()
		.await
}

/// `Scarcity::transfer` arguments.
#[derive(EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
struct TransferCallArgs {
	/// The key the NFT goes to.
	to: [u8; 32],
}

/// `AsScarcity`: the transaction acts as the NFT it names, at the revision it
/// signs over.
#[derive(Clone, Copy, Debug)]
pub struct AsScarcityExtension {
	instance: u64,
	state_nonce: u64,
}

impl From<&NftTransfer> for AsScarcityExtension {
	fn from(nft_transfer: &NftTransfer) -> Self {
		Self { instance: nft_transfer.instance, state_nonce: nft_transfer.state_nonce }
	}
}

/// The pallet's `AsScarcityInfo`: which NFT a transaction acts as.
#[derive(EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub enum AsScarcityInfo {
	/// The NFT `instance` at revision `state_nonce`.
	AsNft {
		/// The instance.
		instance: u64,
		/// Its revision.
		state_nonce: u64,
	},
}

impl TransactionExtension for AsScarcityExtension {
	type Implicit = ();
	type Explicit = Option<AsScarcityInfo>;

	fn name(&self) -> &str {
		"AsScarcity"
	}

	fn implicit(&self) {}

	fn explicit(
		&self,
		_implication: &InheritedImplication,
	) -> Result<Option<AsScarcityInfo>, ExtensionError> {
		let Self { instance, state_nonce } = *self;
		Ok(Some(AsScarcityInfo::AsNft { instance, state_nonce }))
	}
}

#[cfg(test)]
mod tests;
