//! JAMNP-S helpers for hosts that implement `JamPeerTransport`.
//!
//! Peer access is a runtime permission, not a manifest capability: before a
//! `dial` connects, the host requires
//! [`RemotePermission::JamPeers`](truapi::latest::RemotePermission::JamPeers)
//! for the requested genesis, reading the product's stored decision, prompting
//! when it is undetermined and persisting the answer per product and genesis.
//! Anything short of a grant answers
//! [`NotGranted`](truapi::latest::HostJamPeerTransportDialError::NotGranted).
//!
//! The host also owns the transport: it builds the JAMNP-S ALPN from the
//! genesis ([`alpn`]), verifies the peer certificate against the identity the
//! guest named, frames messages and enforces the `JAM_PEER_TRANSPORT_MAX_*` caps
//! from `truapi::latest`.

use core::fmt;

/// JAMNP-S ALPN prefix; the suffix is the first eight hex nibbles of the genesis
/// header hash.
pub const ALPN_PREFIX: &str = "jamnp-s/1/";

/// A genesis spelling other than 32 bytes of lowercase hex.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("JAM genesis must be 32 bytes of lowercase hex")]
pub struct InvalidGenesis;

/// The JAMNP-S ALPN protocol id for `genesis`:
/// `jamnp-s/1/<first 8 hex nibbles of the genesis header hash>`.
pub fn alpn(genesis: &[u8; 32]) -> String {
    let mut alpn = String::with_capacity(ALPN_PREFIX.len() + 8);
    alpn.push_str(ALPN_PREFIX);
    for byte in &genesis[..4] {
        use fmt::Write as _;
        write!(alpn, "{byte:02x}").expect("String never fails to write");
    }
    alpn
}

/// Parse a genesis header hash: exactly 64 lowercase hex digits, with or
/// without a `0x` prefix. Uppercase is refused so one hash has one spelling.
pub fn parse_genesis(text: &str) -> Result<[u8; 32], InvalidGenesis> {
    let hex = text.strip_prefix("0x").unwrap_or(text);
    if hex.len() != 64 {
        return Err(InvalidGenesis);
    }
    let mut genesis = [0u8; 32];
    for (index, pair) in hex.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        let nibble = |byte: u8| match byte {
            b'0'..=b'9' => Ok(byte - b'0'),
            b'a'..=b'f' => Ok(byte - b'a' + 10),
            _ => Err(InvalidGenesis),
        };
        genesis[index] = (nibble(pair[0])? << 4) | nibble(pair[1])?;
    }
    Ok(genesis)
}
