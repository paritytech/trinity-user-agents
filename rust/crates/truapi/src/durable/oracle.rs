//! How a domain tells the engine what it can see on chain.

mod completion;
mod monotone;
mod registry;
mod unobservable;

pub use completion::{CompletionOracle, LedgerView, PassScope};
pub use monotone::{Monotone, MonotoneEffect};
pub use registry::DurableRegistry;
pub use unobservable::Unobservable;
