//! Layer 1 of NFT purses: purse records in the core database, independent of
//! products and permissions.
//!
//! A caller tags each purse when creating it and finds its purses again by
//! that tag. Layer 1 stores and matches tags but never reads them.

mod dao;
mod purse;

pub use purse::{CreatePurseError, NftLedger, PurseId, PurseRecord, PurseTag};
