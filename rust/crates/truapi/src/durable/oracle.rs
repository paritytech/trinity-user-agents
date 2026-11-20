//! How a domain tells the engine what it can see on chain.

mod completion;
mod registry;
mod unobservable;

pub use completion::{CompletionOracle, PassScope};
pub use registry::DurableRegistry;
pub use unobservable::Unobservable;
