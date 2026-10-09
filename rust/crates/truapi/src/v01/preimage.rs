use parity_scale_codec::{Decode, Encode};

/// Preimage submission error.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum PreimageSubmitError {
    /// Catch-all.
    Unknown {
        /// Human-readable failure reason.
        reason: String,
    },
}

/// Request to subscribe to preimage lookup results.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemotePreimageLookupSubscribeRequest {
    /// Hash of the preimage.
    pub key: Vec<u8>,
}

/// Item containing an optional preimage lookup result.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemotePreimageLookupSubscribeItem {
    /// Preimage data, if found.
    pub value: Option<Vec<u8>>,
}

/// Request to read a preimage once, through a chosen route.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemotePreimageReadRequest {
    /// Hash of the preimage: the blake2b-256 of the value.
    pub key: Vec<u8>,
    /// Where the host reads.
    pub route: PreimageReadRoute,
    /// `false` lets the host answer from its own caches, and the report then
    /// says so. `true` makes the host read through the route, so a product can
    /// measure the network.
    pub skip_host_caches: bool,
}

/// Where the host reads a preimage.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum PreimageReadRoute {
    /// The host's normal order, as for a lookup.
    Auto,
    /// Bulletin only, never a cache provider.
    Bulletin,
    /// Cache providers only, in the host's order, with no Bulletin fallback.
    Cache,
    /// This cache provider only.
    CacheProvider {
        /// Endpoint id of the cache provider.
        id: [u8; 32],
    },
}

/// The value of a preimage read, and how the host got it.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemotePreimageReadResponse {
    /// The value, checked against the key. `None` when no source on the route
    /// had it.
    pub value: Option<Vec<u8>>,
    /// How the host got the value, or why it did not.
    pub report: PreimageReadReport,
}

/// How a host read a preimage.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct PreimageReadReport {
    /// The source of the value. `None` when no source had it.
    pub served_by: Option<PreimageReadSource>,
    /// Every source that the host asked, in order, the one that served included.
    pub attempts: Vec<PreimageReadAttempt>,
    /// Time from the request to the response inside the host, in milliseconds.
    pub host_ms: u32,
}

/// The source that served a preimage read.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum PreimageReadSource {
    /// A cache of the host itself: the core's read-after-write cache or a
    /// platform cache.
    HostCache,
    /// A cache provider.
    CacheProvider {
        /// Endpoint id of the cache provider.
        id: [u8; 32],
        /// Display name of the provider, from the provider set.
        name: Option<String>,
        /// Region of the provider, from the provider set.
        region: Option<String>,
        /// Where the provider got the content.
        origin: CacheOrigin,
        /// Position of the provider in the host's order for this read, from 0.
        rank: u32,
        /// Whether the provider is a home node of the key.
        home: bool,
        /// Time that the provider reports for its own work, in milliseconds.
        provider_ms: Option<u32>,
        /// The provider's trace of its steps, as JSON. The host passes it on
        /// without reading it.
        trace: Option<String>,
    },
    /// Bulletin, through the host's own Bulletin client.
    Bulletin {
        /// How the host reached Bulletin.
        via: BulletinReadVia,
    },
}

/// Where a cache provider got the content of a read.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum CacheOrigin {
    /// From its own store: the provider had the content already.
    Local,
    /// From another cache provider.
    Peer {
        /// Endpoint id of the other provider.
        id: [u8; 32],
    },
    /// From Bulletin, during this read.
    Source,
    /// The provider did not say.
    Unknown,
}

/// How a host reaches Bulletin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum BulletinReadVia {
    /// `bitswap_v1_get` over the JSON-RPC of a Bulletin node.
    Rpc,
    /// Bitswap through a light client or an IPFS node.
    Bitswap,
    /// An IPFS HTTP gateway.
    Gateway,
}

/// One source that a host asked during a preimage read.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct PreimageReadAttempt {
    /// The source that the host asked.
    pub source: PreimageReadAttemptSource,
    /// What the source answered.
    pub outcome: PreimageReadOutcome,
    /// Time of this attempt, in milliseconds.
    pub ms: u32,
}

/// A source that a host asked during a preimage read.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum PreimageReadAttemptSource {
    /// A cache of the host itself.
    HostCache,
    /// A cache provider.
    CacheProvider {
        /// Endpoint id of the cache provider.
        id: [u8; 32],
    },
    /// Bulletin, through the host's own Bulletin client.
    Bulletin {
        /// How the host reached Bulletin.
        via: BulletinReadVia,
    },
}

/// What one source answered during a preimage read.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum PreimageReadOutcome {
    /// The source sent the value.
    Served,
    /// The source does not have the value.
    Miss,
    /// The bytes did not hash to the key. The host dropped them and paid nothing.
    BadBytes,
    /// The source refused, for example a provider that the payer cannot pay.
    Refused {
        /// Human-readable reason.
        reason: String,
    },
    /// The source failed: unreachable, timed out, or an error.
    Failed {
        /// Human-readable reason.
        reason: String,
    },
}

/// Why a host could not read a preimage.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum PreimageReadError {
    /// The route names a cache provider that is not in the host's provider set.
    UnknownProvider,
    /// The route needs cache providers, and the host has none.
    NoCacheProviders,
    /// Catch-all.
    Unknown {
        /// Human-readable failure reason.
        reason: String,
    },
}
