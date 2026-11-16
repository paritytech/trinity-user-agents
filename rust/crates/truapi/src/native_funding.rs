//! The native funding route: top-ups and withdrawals on the stack's own
//! network, run by the core itself with no provider worker.
//!
//! [`xcm`] holds the XCM v5 programs the route sends with
//! `PolkadotXcm.execute`.

pub mod xcm;
