//! A genesis-bound JAM peer-transport grant, not a general network capability.
//!
//! The app manifest (`$v` 2) declares `capabilities.network.jam = { genesis }`.
//! A host that honours the declaration constructs a [`PeerTransportGrant`]
//! from the manifest it actually loaded, never from a guest request, and
//! accepts `PeerTransport::dial` only for that genesis. Everything else stays
//! [`NotGranted`](truapi::latest::HostPeerTransportDialError::NotGranted),
//! and the grant is revoked when the execution stops.
//!
//! The host also owns the transport: it builds the JAMNP-S ALPN from the
//! genesis ([`PeerTransportGrant::alpn`]), verifies the peer certificate
//! against the identity the guest named, frames messages and enforces the
//! `PEER_TRANSPORT_MAX_*` caps from `truapi::latest`.

use core::fmt;

/// Manifest schema version that carries `capabilities.network.jam`.
pub const MANIFEST_SCHEMA_VERSION: u64 = 2;
/// JAMNP-S ALPN prefix; the suffix is the first eight hex nibbles of the genesis
/// header hash.
pub const ALPN_PREFIX: &str = "jamnp-s/1/";

/// Explicit genesis-bound authority derived from the loaded manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerTransportGrant {
    /// Genesis header hash the guest may dial peers of.
    pub genesis: [u8; 32],
}

/// Invalid manifest capability. No grant is created on failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PeerTransportGrantError {
    /// The manifest is not a JSON object or is not schema version 2.
    #[error("manifest must be a schema-version-2 JSON object")]
    InvalidManifest,
    /// `capabilities.network.jam` is present but not an object with a `genesis`.
    #[error("capabilities.network.jam must be an object with a genesis")]
    InvalidCapability,
    /// The genesis is not a 32-byte lowercase hex hash.
    #[error("capabilities.network.jam.genesis must be 32 bytes of lowercase hex")]
    InvalidGenesis,
}

impl PeerTransportGrant {
    /// Read the grant from the loaded manifest bytes.
    ///
    /// `Ok(None)` means the manifest declares no JAM network capability, so the
    /// host must leave every `PeerTransport` method at its `NotGranted` default.
    /// Unknown manifest fields are ignored; only the capability itself is
    /// validated here.
    pub fn from_manifest(manifest_json: &[u8]) -> Result<Option<Self>, PeerTransportGrantError> {
        let manifest: serde_json::Value = serde_json::from_slice(manifest_json)
            .map_err(|_| PeerTransportGrantError::InvalidManifest)?;
        let object = manifest
            .as_object()
            .ok_or(PeerTransportGrantError::InvalidManifest)?;
        if object.get("$v").and_then(serde_json::Value::as_u64) != Some(MANIFEST_SCHEMA_VERSION) {
            return Err(PeerTransportGrantError::InvalidManifest);
        }
        let Some(jam) = object
            .get("capabilities")
            .and_then(|capabilities| capabilities.get("network"))
            .and_then(|network| network.get("jam"))
        else {
            return Ok(None);
        };
        let genesis = jam
            .as_object()
            .and_then(|jam| jam.get("genesis"))
            .ok_or(PeerTransportGrantError::InvalidCapability)?
            .as_str()
            .ok_or(PeerTransportGrantError::InvalidGenesis)?;
        Ok(Some(Self {
            genesis: parse_genesis(genesis)?,
        }))
    }

    /// Whether a `dial` naming `genesis` is within this grant.
    pub fn permits(&self, genesis: &[u8; 32]) -> bool {
        self.genesis == *genesis
    }

    /// The JAMNP-S ALPN protocol id for the granted genesis.
    pub fn alpn(&self) -> String {
        alpn(&self.genesis)
    }
}

impl fmt::Display for PeerTransportGrant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "jam:")?;
        for byte in self.genesis {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

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

/// Parse a manifest genesis: exactly 64 lowercase hex digits, with or without a
/// `0x` prefix. Uppercase is refused so one hash has one spelling.
pub fn parse_genesis(text: &str) -> Result<[u8; 32], PeerTransportGrantError> {
    let hex = text.strip_prefix("0x").unwrap_or(text);
    if hex.len() != 64 {
        return Err(PeerTransportGrantError::InvalidGenesis);
    }
    let mut genesis = [0u8; 32];
    for (index, pair) in hex.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        let nibble = |byte: u8| match byte {
            b'0'..=b'9' => Ok(byte - b'0'),
            b'a'..=b'f' => Ok(byte - b'a' + 10),
            _ => Err(PeerTransportGrantError::InvalidGenesis),
        };
        genesis[index] = (nibble(pair[0])? << 4) | nibble(pair[1])?;
    }
    Ok(genesis)
}
