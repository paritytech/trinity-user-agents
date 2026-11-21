//! Light specs shared by the crate's tests, registered under made-up genesis
//! hashes: the registry keys a chain by whatever hash its host names.

/// Real relay-chain spec (checkpoint included) vendored from smoldot's
/// demo specs: `add_chain` and spec-local JSON-RPC queries succeed without
/// any network access.
pub const RELAY_SPEC: &str = include_str!("../tests/fixtures/paseo.json");

/// Parachain of [`RELAY_SPEC`], used to exercise the relay-add path.
pub const PARACHAIN_SPEC: &str = include_str!("../tests/fixtures/paseo_people.json");

pub const RELAY_GENESIS: [u8; 32] = [1; 32];
pub const PARACHAIN_GENESIS: [u8; 32] = [2; 32];
