//! An NFT transfer, fee-less or paid, signed by the item key it transfers from.

use subxt::{ext::scale_encode::EncodeAsType, utils::H256};

use crate::{
	chain::{
		ExtensionError, ExtrinsicBuildError, ExtrinsicService, InheritedImplication,
		MortalExtrinsic, TransactionExtension,
	},
	host_internal::extrinsic::Sr25519Signer,
	nft::ItemKey,
};

/// One NFT transfer: the instance at the source key, the revision it must sign,
/// and the key it goes to.
#[derive(Clone)]
pub struct NftTransfer {
	/// The instance the source key holds.
	pub instance: u64,
	/// The instance's `state_nonce` now, which the transfer signs over.
	pub state_nonce: u64,
	/// The key it goes to.
	pub destination: ItemKey,
}

impl ExtrinsicService {
	/// `nft_transfer` on the chain `genesis`, signed by `signer` and born at
	/// the block this service picks. It pays no fee and uses no account nonce.
	pub async fn sign_nft_transfer(
		&self,
		genesis: H256,
		signer: &Sr25519Signer,
		nft_transfer: &NftTransfer,
	) -> Result<MortalExtrinsic, ExtrinsicBuildError> {
		self.build_mortal_extrinsic(genesis)
			.with_call_args(
				"Scarcity",
				"transfer",
				TransferCallArgs { to: nft_transfer.destination.0 },
			)
			.with_extension(AsScarcityExtension::from(nft_transfer))
			.with_signer(signer)
			.with_nonce(0)
			.build()
			.await
	}

	/// `nft_transfer` as the paid `Scarcity::transfer_by_holder` on the chain
	/// `genesis`, signed by `signer` with its account nonce and born at the
	/// block this service picks. The runtime takes the fee from the signer's
	/// PGAS. It names no revision, so `state_nonce` is not signed over.
	pub async fn sign_paid_nft_transfer(
		&self,
		genesis: H256,
		signer: &Sr25519Signer,
		nft_transfer: &NftTransfer,
	) -> Result<MortalExtrinsic, ExtrinsicBuildError> {
		let args = PaidTransferCallArgs {
			instance: nft_transfer.instance,
			to: nft_transfer.destination.0,
		};
		self.build_mortal_extrinsic(genesis)
			.with_call_args("Scarcity", "transfer_by_holder", args)
			.with_signer(signer)
			.build()
			.await
	}
}

/// `Scarcity::transfer` arguments.
#[derive(EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
struct TransferCallArgs {
	/// The key the NFT goes to.
	to: [u8; 32],
}

/// `Scarcity::transfer_by_holder` arguments.
#[derive(EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
struct PaidTransferCallArgs {
	/// The instance the signer holds.
	instance: u64,
	/// The key the NFT goes to.
	to: [u8; 32],
}

/// `AsScarcity`: the transaction acts as the NFT it names, at the revision it
/// signs over.
#[derive(Clone)]
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
