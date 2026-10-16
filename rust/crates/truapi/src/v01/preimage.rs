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
	/// Every source that the host asked, in order. The list includes the source
	/// that served.
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
		/// The trace of the steps of the provider, as JSON. The host does not
		/// read it and gives it to the product as it is.
		trace: Option<String>,
		/// The price of the read that the provider gave, in the units of its
		/// ledger. The host pays it with the receipt for this read.
		price: Option<u64>,
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
	/// The bytes did not hash to the key. The host dropped them and paid
	/// nothing.
	BadBytes,
	/// The source refused, for example a provider that the payer cannot pay.
	Refused {
		/// Human-readable reason.
		reason: String,
	},
	/// The source failed: the host could not reach it, the request took too
	/// long, or the source answered with an error.
	Failed {
		/// Human-readable reason.
		reason: String,
	},
}

/// Request to read one file of a BitTorrent torrent through cache providers.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemotePreimageReadTorrentRequest {
	/// A BitTorrent v1 magnet link: `magnet:?xt=urn:btih:…`. It can select a
	/// file with `so=` and give peers with `x.pe`.
	pub magnet: String,
	/// The index of the file in the torrent. `None` uses `so=` of the magnet
	/// link, or file 0, the file of a single-file torrent.
	pub file: Option<u32>,
	/// Where the host reads. `Bulletin` is not a route for a torrent.
	pub route: PreimageReadRoute,
	/// A part of the file. `None` asks for the whole file. The host answers a large file in
	/// parts only: then it gives no value, and the product reads each part with a range.
	pub range: Option<TorrentReadRange>,
}

/// A part of the file of a torrent read: `length` bytes from `offset`. The host can give
/// fewer bytes, at the end of the file or above its own limit for one answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct TorrentReadRange {
	/// The first byte of the part in the file.
	pub offset: u64,
	/// The number of bytes of the part.
	pub length: u32,
}

/// The file of a torrent read, and how the host got it.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemotePreimageReadTorrentResponse {
	/// The file, or the part of the request, checked against the piece hashes of
	/// the torrent. `None` when no source on the route had it, while a provider
	/// downloads it (`progress`), or for a large file that the product reads in
	/// parts (`size` is then set).
	pub value: Option<Vec<u8>>,
	/// The preimage key of the file: the blake2b-256 of the whole file, so a
	/// product can read the file again with `Preimage.read`. For a whole value
	/// the core derives it from the value. For a part, or a large file without a
	/// value, the host derives it from the file that it checked.
	pub key: Option<[u8; 32]>,
	/// The size of the whole file, when the host has the file.
	pub size: Option<u64>,
	/// The name of the file in the torrent, when the host knows it.
	pub name: Option<String>,
	/// The index of the file in the torrent.
	pub file: u32,
	/// How the host got the file, or why it did not.
	pub report: PreimageReadReport,
	/// While a cache provider still downloads the file: what it did so far. `value` is then
	/// `None`, and the product calls `readTorrent` again, for the file or a newer progress.
	pub progress: Option<TorrentReadProgress>,
	/// The files of the torrent, once the host has its metadata. The product can read each one
	/// with its index. Padding files (BEP 47) are not in the list.
	pub files: Vec<TorrentReadFile>,
}

/// One file of a torrent.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct TorrentReadFile {
	/// The index of the file in the torrent, for the `file` of a read.
	pub index: u32,
	/// The path of the file in the torrent, with `/` between its parts.
	pub path: String,
	/// The size of the file in bytes.
	pub size: u64,
}

/// What a cache provider did so far for a torrent file that it still downloads.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct TorrentReadProgress {
	/// Endpoint id of the cache provider that downloads the file.
	pub provider: [u8; 32],
	/// The step of the download.
	pub state: TorrentReadState,
	/// The size of the file, once the provider has the metadata of the torrent.
	pub size: Option<u64>,
	/// The bytes of the file that the provider has.
	pub downloaded: u64,
	/// The peers that the provider is connected to.
	pub peers: u32,
	/// Time since the download started, in milliseconds.
	pub elapsed_ms: u64,
}

/// The step of a torrent download at a cache provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum TorrentReadState {
	/// The provider starts: it looks in its store and asks other providers.
	Starting,
	/// The provider asks peers for the metadata of the torrent.
	Metadata,
	/// The provider downloads the pieces of the file.
	Downloading,
	/// The pieces are complete, and the provider stores the file.
	Importing,
	/// The host receives the file from the provider and checks it against the torrent.
	Receiving,
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

/// Request for the cache account of the payer that the host uses for this product.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemotePreimageCacheAccountRequest {
	/// The cache provider to ask. `None` lets the host pick one from its order
	/// for reads.
	pub provider: Option<[u8; 32]>,
}

/// The cache account of the payer of this product, as the ledger of a cache
/// provider gives it, and the prices of that ledger. The units are those of the
/// ledger.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemotePreimageCacheAccountResponse {
	/// The public key (sr25519) with which the host pays cache providers for
	/// this product.
	pub payer: [u8; 32],
	/// Endpoint id of the cache provider that answered.
	pub provider: [u8; 32],
	/// What is left today of the free allowance of the payer.
	pub allowance: u64,
	/// The part of `allowance` that retention can still use today.
	pub allowance_retention: u64,
	/// The paid credits of the payer.
	pub paid: u64,
	/// The allowance of each new day.
	pub allowance_per_day: u64,
	/// The part of the daily allowance that retention can use.
	pub retention_allowance_per_day: u64,
	/// The price of one read.
	pub read_price: u64,
	/// The price of retention for each started MiB and each started hour.
	pub retention_price: u64,
}

/// Request to keep a preimage on a cache provider for a period, paid in advance.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemotePreimageRetainRequest {
	/// The preimage key: blake2b-256 of the value. For a chunked file, the key of
	/// its manifest.
	pub key: Vec<u8>,
	/// How long to keep the preimage, in seconds from now.
	pub period_secs: u64,
	/// The size that the retention pays for. For a chunked file, the size of the
	/// manifest and all its chunks, so the provider keeps the chunks too. `None`:
	/// the host reads the value to learn its size.
	pub size: Option<u64>,
	/// The cache provider to keep it. `None` lets the host pick one, a home node
	/// of the key first.
	pub provider: Option<[u8; 32]>,
}

/// The retention that the host bought.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemotePreimageRetainResponse {
	/// Unix seconds until which the provider keeps the preimage.
	pub until: u64,
	/// Endpoint id of the provider that keeps it.
	pub provider: [u8; 32],
	/// Display name of the provider, from the provider set.
	pub name: Option<String>,
	/// Region of the provider, from the provider set.
	pub region: Option<String>,
	/// The size that the retention paid for.
	pub size: u64,
	/// The chunks of a chunked file that the provider keeps with the manifest.
	pub chunks: u32,
	/// What the retention cost, in the units of the ledger of the provider. A
	/// retention that a previous one covers costs 0.
	pub charged: u64,
}

/// Why the host could not keep a preimage.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum PreimageRetainError {
	/// No source has the preimage yet. The product can ask again after its
	/// submit lands.
	NotFound,
	/// The period is longer than the providers accept.
	PeriodTooLong {
		/// The longest period, in seconds.
		max_secs: u64,
	},
	/// The payer cannot pay for the period.
	NotAvailable,
	/// The person declined the retention.
	Declined,
	/// Catch-all.
	Unknown {
		/// Human-readable failure reason.
		reason: String,
	},
}

/// What a product asks a cache provider to keep, or names to order the providers.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum PreimageTarget {
	/// A preimage key: blake2b-256 of the value. For a chunked file, the key of
	/// its manifest.
	Key(Vec<u8>),
	/// A CID that Bulletin has, as text: for example the storage root of a site
	/// that `bulletin-deploy` published (dag-pb, sha2-256).
	Cid(String),
	/// A DotNS name, such as `site.paseo`. The host resolves it to the CID of its
	/// contenthash, which is the CID that the host loads for the site.
	DotNs(String),
}

/// Request for the cache providers of the host.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemotePreimageCacheProvidersRequest {
	/// The content that the order is for. The home providers of the target come
	/// first. `None` gives the order for reads in general.
	pub target: Option<PreimageTarget>,
}

/// Where the host found a cache provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum CacheProviderSource {
	/// The provider registry, or the provider set of the host.
	Registry,
	/// A provider that the person added in the host.
	User,
}

/// One cache provider of the host, as a person can pick it.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct CacheProviderInfo {
	/// Endpoint id of the provider.
	pub id: [u8; 32],
	/// Display name of the provider.
	pub name: Option<String>,
	/// Region of the provider, such as a city.
	pub region: Option<String>,
	/// The operator of the provider.
	pub operator: Option<String>,
	/// Where the host found the provider.
	pub source: CacheProviderSource,
	/// The provider is a home provider of the target.
	pub home: bool,
	/// The time that the host measured to the provider, in milliseconds.
	pub latency_ms: Option<u32>,
	/// A read from the provider failed a short time ago.
	pub failed_recently: bool,
}

/// The cache providers of the host, in its order for the target.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemotePreimageCacheProvidersResponse {
	/// The providers, first the one that the host asks first.
	pub providers: Vec<CacheProviderInfo>,
}

/// Request for the retentions of the payer of this product.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemotePreimageRetentionsRequest {
	/// The cache provider to ask for the ledger. `None` lets the host pick one.
	pub provider: Option<[u8; 32]>,
}

/// The state of a retention on its cache provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum RetentionState {
	/// The provider holds every piece.
	Kept,
	/// The provider fetches the pieces.
	Fetching {
		/// The pieces that the provider has.
		pieces: u32,
		/// All the pieces.
		total: u32,
	},
	/// The retention is paid, and the provider does not hold the content.
	Lost,
	/// The retention ended.
	Ended,
	/// The provider did not answer.
	Unknown,
}

/// One retention of the payer: what the ledger charged and what the provider holds.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct PreimageRetention {
	/// The CID of the content, as text.
	pub cid: String,
	/// Endpoint id of the provider that keeps it.
	pub provider: [u8; 32],
	/// Display name of the provider.
	pub name: Option<String>,
	/// Region of the provider.
	pub region: Option<String>,
	/// The operator of the provider.
	pub operator: Option<String>,
	/// The size that the retention pays for.
	pub size: u64,
	/// The pieces that the provider keeps: the root and its chunks.
	pub pieces: u32,
	/// Unix seconds of the start of the retention.
	pub from: u64,
	/// Unix seconds of the end of the retention: the end that the payer chose,
	/// or for a retention that renews, the end of the paid days.
	pub until: u64,
	/// Unix seconds until which the retention is paid.
	pub paid_until: u64,
	/// All that the retentions of this content on this provider cost.
	pub charged: u64,
	/// The price of one day of a retention that the provider charges one day at
	/// a time. 0 for a retention paid in advance.
	pub per_day: u64,
	/// The provider renews the retention each day until the payer stops it.
	pub renews: bool,
	/// The state on the provider.
	pub state: RetentionState,
}

/// The retentions of the payer of this product.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemotePreimageRetentionsResponse {
	/// The public key (sr25519) of the payer.
	pub payer: [u8; 32],
	/// The retentions, the latest end first.
	pub retentions: Vec<PreimageRetention>,
}

/// Request to stop the renewal of a retention.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemotePreimageStopRetentionRequest {
	/// The CID of the content, as text.
	pub cid: String,
	/// Endpoint id of the provider that keeps it.
	pub provider: [u8; 32],
}

/// The end of a stopped retention.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemotePreimageStopRetentionResponse {
	/// Unix seconds: the provider keeps the content until the end of the paid day.
	pub until: u64,
}
