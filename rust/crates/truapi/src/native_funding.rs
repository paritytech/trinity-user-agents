//! The native funding route: top-ups on the stack's own network, run by the
//! core itself with no provider worker.
//!
//! The route serves sessions as a built-in funding provider under
//! [`ProductId::native_funding`](crate::platform::ProductId::native_funding):
//! [`session::NativeLink`] is the [`FundingProviderLink`](crate::runtime::FundingProviderLink)
//! the registry calls, [`chain::NativeChain`] the chain work it does, and
//! [`xcm`] the programs it sends. The apps keep their CASH engines: the route
//! credits through the host's top-up with a session key as the source.

pub mod chain;
pub mod keys;
pub mod session;
pub mod tokens;
pub mod xcm;
