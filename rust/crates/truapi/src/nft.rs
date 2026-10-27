//! Layer 1 of NFT purses: purse records and their item keys in the core
//! database, independent of products and permissions.
//!
//! A caller tags each purse when creating it and finds its purses again by
//! that tag. Layer 1 stores and matches tags but never reads them.

mod dao;
mod handoff;
mod holding;
mod import;
mod imported_secrets;
mod item;
mod keys;
mod ledger;
#[cfg(test)]
mod live;
mod operation;
mod purse;
mod scarcity;
mod standing;
#[cfg(test)]
mod testing;

pub use handoff::{HandoffError, HandoffRequest, HandoffTag};
pub use holding::HeldItem;
pub use import::{ImportRequest, StartImportError};
pub use imported_secrets::ImportedSecretStore;
pub use item::{AllocateItemsError, PageRequest};
pub use keys::{
	ImportedItemKey, InvalidItemSecret, ItemKey, ItemKeyDeriver, ItemSecret, ItemSecrets,
	ItemSigner, ItemSlot, item_keys_from_entropy, item_secrets_from_entropy,
	item_signer_from_entropy,
};
pub use ledger::{NftLedger, NftLedgerDeps};
pub use operation::{
	OperationKey, StartOperationError, TransferDestinations, TransferProgress, TransferRequest,
	TransferState,
};
pub use purse::{CreatePurseError, PurseId, PurseRecord, PurseTag, UnknownPurse};
pub use scarcity::{
	HoldingTracker, HoldingTrackerDeps, ScarcityTransfersDeps, with_scarcity_transfers,
};
pub use standing::ItemRefusal;
#[cfg(test)]
pub use testing::{TEST_INSTALLATION, insert_item_row, test_ledger};
