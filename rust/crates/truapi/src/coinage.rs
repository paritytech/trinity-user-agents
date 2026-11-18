//! The coinage layer: a wallet of coins and recycler entries over the coinage
//! pallet, built from the design in `docs/design/coinage/`.
//!
//! Each public submodule is one component of `coinage-layer-components.md`
//! and names it in its documentation. The layer serves exactly one coinage
//! instance on the People chain and reaches the chain only through the
//! interfaces the design's section 1 lists.

pub mod chain;
pub mod denominations;
pub mod namespace;
pub mod params;
pub mod policy;
pub mod split;

#[cfg(test)]
pub mod testing;

pub use denominations::Exponent;
pub use namespace::{CoinSecret, PurseId};
pub use params::{ChainConfig, ChainParams};
pub use policy::{HeldBackSpend, Initiator, MemberThreshold, PrivacyPolicy, ReadyRule, RecycleAge};
