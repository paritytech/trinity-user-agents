//! Layer 1 of NFT purses: purse records and their item keys in the core
//! database, independent of products and permissions.
//!
//! A caller tags each purse when creating it and finds its purses again by
//! that tag. Layer 1 stores and matches tags but never reads them.

mod dao;
mod holding;
mod item;
mod keys;
mod ledger;
mod purse;
mod scarcity;
#[cfg(test)]
mod testing;

pub use holding::HeldItem;
pub use item::{AllocateItemsError, ItemPage};
pub use keys::{ItemKey, ItemKeyDeriver, ItemSlot, item_keys_from_entropy};
pub use ledger::{NftLedger, NftLedgerDeps};
pub use purse::{CreatePurseError, PurseId, PurseRecord, PurseTag};
pub use scarcity::{HoldingTracker, HoldingTrackerDeps};
#[cfg(test)]
pub use testing::{TEST_INSTALLATION, test_ledger};
