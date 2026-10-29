//! What the Scarcity pallet holds for each item key. Everything that knows
//! the pallet's storage layout stays in here.

mod chain;
mod tracker;

pub use tracker::{HoldingTracker, HoldingTrackerDeps};
