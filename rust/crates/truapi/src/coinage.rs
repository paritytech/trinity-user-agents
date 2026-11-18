//! The coinage layer: a wallet of coins and recycler entries over the coinage
//! pallet, built from the design in `docs/design/coinage/`.
//!
//! Each public submodule is one component of `coinage-layer-components.md`
//! and names it in its documentation. The layer serves exactly one coinage
//! instance on the People chain and reaches the chain only through the
//! interfaces the design's section 1 lists.

pub mod assembly;
pub mod balance;
pub mod chain;
pub mod classification;
pub mod denominations;
pub mod handover;
pub mod ledger;
pub mod maintenance;
pub mod namespace;
pub mod observation;
pub mod offload;
pub mod operation;
pub mod params;
pub mod planning;
pub mod plugins;
pub mod policy;
pub mod reconstruction;
pub mod registry;
pub mod registration;
pub mod selection;
pub mod split;
pub mod start;
pub mod tokens;

#[cfg(test)]
pub mod testing;

pub use denominations::Exponent;
pub use ledger::{Asset, Snapshot};
pub use namespace::{CoinSecret, PurseId};
pub use params::{ChainConfig, ChainParams};
pub use policy::{HeldBackSpend, Initiator, MemberThreshold, PrivacyPolicy, ReadyRule, RecycleAge};
