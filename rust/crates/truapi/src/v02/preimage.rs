use crate::v01::PreimageTarget;
use parity_scale_codec::{Decode, Encode};

/// The cache account of the payer of this product, as the ledger of a cache
/// provider gives it, and the price list of that ledger with its units.
///
/// v0.2 adds the units of the prices: a read can cost a price for each MiB, and
/// retention can count MiB-days instead of MiB-hours.
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
	/// The price of one read, when `read_price_per_mib` is 0.
	pub read_price: u64,
	/// When not 0, a read costs this for each started MiB, at least one MiB.
	pub read_price_per_mib: u64,
	/// The price of retention for each started MiB and each started unit of
	/// `retention_unit_secs`.
	pub retention_price: u64,
	/// The unit of time of `retention_price` in seconds: 3600 or 86400.
	pub retention_unit_secs: u64,
}

/// One item to keep on a cache provider.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RetainItem {
	/// What to keep.
	pub target: PreimageTarget,
	/// How long to keep it, in seconds from now. With `renew`, the end of the
	/// renewals, at most the longest period of the providers. With `renew`, 0
	/// means no end: the provider renews the retention until the payer stops it.
	pub period_secs: u64,
	/// The size that the retention pays for: the root and all its pieces. `None`:
	/// the host reads the root to learn it.
	pub size: Option<u64>,
	/// The cache provider to keep it. `None` lets the host pick one, a home
	/// provider of the target first.
	pub provider: Option<[u8; 32]>,
	/// The provider charges one day at a time and renews the retention each day
	/// until the payer stops it or `period_secs` ends.
	pub renew: bool,
}

/// Request to keep one or more items on cache providers, paid from the cache
/// payer of the product.
///
/// v0.2 keeps a CID or a DotNS name as well as a preimage key, keeps several
/// items under one confirmation, renews by the day, and answers a dry run
/// with the price.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemotePreimageRetainRequest {
	/// The items to keep.
	pub items: Vec<RetainItem>,
	/// Answer the size, the provider and the price of each item, and keep
	/// nothing. A dry run asks for no permission and no confirmation.
	pub dry_run: bool,
}

/// The kind of content of a retained item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum RetainKind {
	/// One raw block.
	Raw,
	/// A UnixFS file: a dag-pb root with its pieces.
	File,
}

/// The state of a retained item after the call.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum RetainState {
	/// A dry run: nothing was kept or paid.
	Quote,
	/// The provider holds every piece.
	Kept,
	/// The provider fetches the pieces, and charges when it has all of them.
	Fetching {
		/// The id of the pin at the provider.
		pin_id: String,
	},
}

/// What the host did for one item, or would do in a dry run.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RetainOutcome {
	/// The CID of the content, as text. For a DotNS name, the CID of its
	/// contenthash.
	pub cid: String,
	/// Endpoint id of the provider.
	pub provider: [u8; 32],
	/// Display name of the provider.
	pub name: Option<String>,
	/// Region of the provider.
	pub region: Option<String>,
	/// The operator of the provider.
	pub operator: Option<String>,
	/// The kind of content.
	pub kind: RetainKind,
	/// The size that the retention pays for: the root and all its pieces.
	pub size: u64,
	/// The pieces that the provider keeps: the root and its chunks.
	pub pieces: u32,
	/// Unix seconds until which the provider keeps the item. For a retention that
	/// renews, the end of the paid day.
	pub until: u64,
	/// The price of the period on this provider, in the units of its ledger. For
	/// a retention that renews, the price of one day.
	pub price: u64,
	/// What the host paid. 0 in a dry run, and for a period that an earlier
	/// retention covers.
	pub charged: u64,
	/// The part of `price` that the free allowance pays.
	pub free: u64,
	/// The part of `price` that the paid credits pay.
	pub paid: u64,
	/// The part of `price` that the payer cannot pay.
	pub short: u64,
	/// The provider renews the retention each day.
	pub renews: bool,
	/// The state of the item.
	pub state: RetainState,
}

/// What the host did for each item of a retention request, in its order.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemotePreimageRetainResponse {
	/// One outcome for each item of the request.
	pub items: Vec<RetainOutcome>,
}
