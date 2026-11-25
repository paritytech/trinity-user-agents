//! Item keys and where they sit in the derivation tree.

use core::ops::Range;
use std::sync::Weak;

use parity_scale_codec::{Decode, Encode};
use schnorrkel::Keypair;
use subxt::utils::AccountId32;
use zeroize::Zeroizing;

use super::purse::PurseId;
use crate::{
	gap_scan::KeyRange,
	host_internal::extrinsic::Sr25519Signer,
	host_logic::{
		product_account::{ProductAccountError, derive_hard_path_from_keypair},
		root_entropy::RootEntropy,
	},
	installation::InstallationId,
	runtime::AuthorityError,
};

/// Position of an item key under its purse and installation. Allocated in
/// order from 0, because recovery scans from 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Encode, Decode)]
pub struct ItemIndex(pub u32);

/// Public key of one item. Its secret is derived again when it is needed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Encode, Decode)]
pub struct ItemKey(pub [u8; 32]);

impl From<ItemKey> for AccountId32 {
	fn from(key: ItemKey) -> Self {
		Self(key.0)
	}
}

/// Where an item key sits in the derivation tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode)]
pub struct ItemSlot {
	/// Purse the key belongs to.
	pub purse: PurseId,
	/// Installation that allocated it.
	pub installation: InstallationId,
	/// Its position under the purse and installation.
	pub index: ItemIndex,
}

impl ItemSlot {
	/// The hard junctions of `//nft//<purse>//<installation>//<index>`. Every
	/// junction is hard, so a cheque that hands over one item's secret
	/// reveals nothing about its siblings. The installation is written as
	/// `0x` and lowercase hex, as coinage writes it.
	pub fn junctions(&self) -> [String; 4] {
		let [nft, purse, installation] = item_key_prefix(self.purse, self.installation);
		[nft, purse, installation, self.index.0.to_string()]
	}
}

/// The hard junctions `//nft//<purse>//<installation>` every item key of
/// `purse` allocated by `installation` sits under.
fn item_key_prefix(purse: PurseId, installation: InstallationId) -> [String; 3] {
	let installation = format!("0x{}", hex::encode(installation.0));
	["nft".to_string(), purse.0.to_string(), installation]
}

/// Derives item public keys from the user's root entropy.
pub trait ItemKeyDeriver: Send + Sync {
	/// The public key at each of `slots`, in order.
	fn derive_item_keys(&self, slots: &[ItemSlot]) -> Result<Vec<ItemKey>, AuthorityError>;

	/// The public keys of `purse` allocated by `installation` at each of
	/// `indices`, in order. A deriver that holds the seed derives the shared
	/// prefix once for the whole range.
	fn derive_item_key_range(
		&self,
		purse: PurseId,
		installation: InstallationId,
		indices: Range<u32>,
	) -> Result<Vec<ItemKey>, AuthorityError> {
		let slots: Vec<ItemSlot> = indices
			.map(|index| ItemSlot { purse, installation, index: ItemIndex(index) })
			.collect();
		self.derive_item_keys(&slots)
	}
}

/// For a deriver that owns the ledger, which must not keep it alive in turn.
/// Once the deriver is gone there are no keys to derive.
impl<Deriver: ItemKeyDeriver> ItemKeyDeriver for Weak<Deriver> {
	fn derive_item_keys(&self, slots: &[ItemSlot]) -> Result<Vec<ItemKey>, AuthorityError> {
		let Some(deriver) = self.upgrade() else {
			let reason = "the item key deriver was dropped".to_string();
			return Err(AuthorityError::Unavailable { reason });
		};
		deriver.derive_item_keys(slots)
	}

	fn derive_item_key_range(
		&self,
		purse: PurseId,
		installation: InstallationId,
		indices: Range<u32>,
	) -> Result<Vec<ItemKey>, AuthorityError> {
		let Some(deriver) = self.upgrade() else {
			let reason = "the item key deriver was dropped".to_string();
			return Err(AuthorityError::Unavailable { reason });
		};
		deriver.derive_item_key_range(purse, installation, indices)
	}
}

/// The keys recovery scans: one purse's item keys under one installation.
impl KeyRange for dyn ItemKeyDeriver + '_ {
	type PurseId = PurseId;
	type Key = ItemKey;

	fn derive_range(
		&self,
		purse: &PurseId,
		installation: InstallationId,
		indices: Range<u32>,
	) -> anyhow::Result<Vec<ItemKey>> {
		Ok(self.derive_item_key_range(*purse, installation, indices)?)
	}
}

/// Signs with item keys. The secrets never leave the implementor.
pub trait ItemSigner: Send + Sync {
	/// A signer for the key at `slot`. Fails without a session.
	fn item_signer(&self, slot: ItemSlot) -> Result<Sr25519Signer, AuthorityError>;
}

/// For a signer that owns the durable engine, which must not keep it alive
/// in turn. Once the signer is gone there is nothing to sign with.
impl<Signer: ItemSigner> ItemSigner for Weak<Signer> {
	fn item_signer(&self, slot: ItemSlot) -> Result<Sr25519Signer, AuthorityError> {
		let Some(signer) = self.upgrade() else {
			let reason = "the item signer was dropped".to_string();
			return Err(AuthorityError::Unavailable { reason });
		};
		signer.item_signer(slot)
	}
}

/// The 64-byte sr25519 secret of one item key: its scalar, then its nonce.
/// Wiped from memory when dropped.
pub struct ItemSecret(Zeroizing<[u8; 64]>);

impl ItemSecret {
	/// Wraps the 64 bytes of a secret, as `as_bytes` returns them.
	pub fn from_bytes(bytes: Zeroizing<[u8; 64]>) -> Self {
		Self(bytes)
	}

	/// The secret's bytes, as `schnorrkel::SecretKey::from_bytes` reads them.
	pub fn as_bytes(&self) -> &[u8; 64] {
		&self.0
	}
}

/// Bytes that are not an sr25519 secret key.
#[derive(Debug, thiserror::Error)]
#[error(
	"the bytes are not an sr25519 secret key: expected a canonical scalar, then a 32-byte nonce"
)]
pub struct InvalidItemSecret;

/// An item key the host did not derive, with the secret that signs for it.
pub struct ImportedItemKey {
	key: ItemKey,
	secret: ItemSecret,
}

impl ImportedItemKey {
	/// The key `secret` signs for, with that secret. Fails on bytes that are
	/// not a secret key.
	pub fn from_secret(secret: ItemSecret) -> Result<Self, InvalidItemSecret> {
		let parsed =
			schnorrkel::SecretKey::from_bytes(secret.as_bytes()).map_err(|_| InvalidItemSecret)?;
		let key = ItemKey(parsed.to_public().to_bytes());
		Ok(Self { key, secret })
	}

	/// The public key.
	pub fn key(&self) -> ItemKey {
		self.key
	}

	/// The secret that signs for it.
	pub fn secret(&self) -> &ItemSecret {
		&self.secret
	}
}

/// Exports item secrets, so items can be handed off. Only
/// `NftLedger::hand_off` calls it.
pub trait ItemSecrets: Send + Sync {
	/// The secret of each of `slots`, in order. Fails without a session.
	fn derive_item_secrets(&self, slots: &[ItemSlot]) -> Result<Vec<ItemSecret>, AuthorityError>;
}

/// For an exporter that owns the ledger, which must not keep it alive in
/// turn. Once the exporter is gone there are no secrets to export.
impl<Exporter: ItemSecrets> ItemSecrets for Weak<Exporter> {
	fn derive_item_secrets(&self, slots: &[ItemSlot]) -> Result<Vec<ItemSecret>, AuthorityError> {
		let Some(exporter) = self.upgrade() else {
			let reason = "the item secret exporter was dropped".to_string();
			return Err(AuthorityError::Unavailable { reason });
		};
		exporter.derive_item_secrets(slots)
	}
}

impl RootEntropy {
	/// The public key at each of `slots`.
	pub fn item_keys(&self, slots: &[ItemSlot]) -> Result<Vec<ItemKey>, ProductAccountError> {
		let root = self.derive_root_keypair()?;
		let item_key = |slot: &ItemSlot| {
			let keypair = item_keypair_for_slot(root.clone(), slot)?;
			Ok(ItemKey(keypair.public.to_bytes()))
		};
		slots.iter().map(item_key).collect()
	}

	/// The public keys of `purse` allocated by `installation` at each of
	/// `indices`, with the shared prefix derived once.
	pub fn item_key_range(
		&self,
		purse: PurseId,
		installation: InstallationId,
		indices: Range<u32>,
	) -> Result<Vec<ItemKey>, ProductAccountError> {
		let prefix = item_key_prefix(purse, installation);
		let prefix = prefix.each_ref().map(String::as_str);
		let prefix_keypair = self.derive_sr25519_hard_path(&prefix)?;
		indices.map(|index| item_key_for_index(prefix_keypair.clone(), index)).collect()
	}

	/// A signer for the key at `slot`.
	pub fn item_signer(&self, slot: ItemSlot) -> Result<Sr25519Signer, ProductAccountError> {
		let keypair = item_keypair_for_slot(self.derive_root_keypair()?, &slot)?;
		Ok(Sr25519Signer::from_keypair(&keypair))
	}

	/// The secret at each of `slots`.
	pub fn item_secrets(&self, slots: &[ItemSlot]) -> Result<Vec<ItemSecret>, ProductAccountError> {
		let root = self.derive_root_keypair()?;
		let item_secret = |slot: &ItemSlot| {
			let keypair = item_keypair_for_slot(root.clone(), slot)?;
			Ok(ItemSecret(Zeroizing::new(keypair.secret.to_bytes())))
		};
		slots.iter().map(item_secret).collect()
	}
}

/// The keypair at `slot` under `root`.
fn item_keypair_for_slot(root: Keypair, slot: &ItemSlot) -> Result<Keypair, ProductAccountError> {
	let junctions = slot.junctions();
	let junctions = junctions.each_ref().map(String::as_str);
	derive_hard_path_from_keypair(root, &junctions)
}

/// The public key at `prefix_keypair//index`, a hard junction.
fn item_key_for_index(prefix_keypair: Keypair, index: u32) -> Result<ItemKey, ProductAccountError> {
	let junction = index.to_string();
	let keypair = derive_hard_path_from_keypair(prefix_keypair, &[junction.as_str()])?;
	Ok(ItemKey(keypair.public.to_bytes()))
}

#[cfg(test)]
mod tests;
