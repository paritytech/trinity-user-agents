//! Preimage lookup over Bulletin: the blob behind a key, read from a Bulletin node by CID.
//!
//! Transaction storage serves every blob a node retains over bitswap, and the node exposes that
//! on its JSON-RPC as `bitswap_v1_get(cid)`. The CID is fixed by the key: CIDv1, the `raw`
//! codec, and the blake2b-256 multihash that is the preimage key itself. A lookup is therefore
//! one round trip to the network's Bulletin chain, and the host needs no IPFS stack of its own.
//!
//! A subscription emits the current answer at once. On a miss it keeps asking every
//! [`POLL_INTERVAL`] until the blob appears, since a submission from another host lands within
//! a block or two, and it ends once it has delivered a value. A node that cannot be reached is
//! reported as a miss as well, never as the end of the subscription, so a caller waiting on a
//! blob survives a transport failure. A request the node can never answer, such as an invalid
//! CID or a node without `bitswap_v1_get`, ends the subscription with an error after the miss.
//! Every value is checked against the key before it is emitted.

use core::time::Duration;
use std::sync::{Arc, Weak};

use futures::{
	future::{self, Either, FutureExt},
	pin_mut,
	stream::{self, BoxStream, StreamExt},
};
use tracing::{debug, info, warn};
use truapi::latest as api;

use crate::{
	host_internal::bulletin::{BitswapError, BulletinCid, bitswap_get},
	platform::PreimageHost,
	runtime::RuntimeServices,
	time::{RealTimer, Timer},
};

/// How long a miss waits before asking the node again: about one Bulletin block.
const POLL_INTERVAL: Duration = Duration::from_secs(6);
/// Bound on connecting to the node and one round trip to it over the core's own connection.
const RPC_TIMEOUT: Duration = Duration::from_secs(10);

/// Why a source could not answer.
pub enum SourceError {
	/// Worth asking again: the node was unreachable, busy, or dropped the connection.
	Transient(String),
	/// The node can never answer this request, so the lookup stops.
	Permanent(String),
}

/// Where blobs are fetched from, by CID. The seam the subscription logic is tested through.
#[async_trait::async_trait]
pub trait BlobSource: Send + Sync {
	/// The blob stored under `cid`, or `None` when the source does not hold it.
	async fn get(&self, cid: &BulletinCid) -> Result<Option<Vec<u8>>, SourceError>;
}

/// Blobs over the Bulletin connection the core already holds for preimage submission. Held
/// weakly, so a lookup in flight does not keep the services once the runtime is gone.
#[async_trait::async_trait]
impl BlobSource for Weak<RuntimeServices> {
	async fn get(&self, cid: &BulletinCid) -> Result<Option<Vec<u8>>, SourceError> {
		let Some(services) = self.upgrade() else {
			return Err(SourceError::Permanent("the runtime is gone".to_string()));
		};
		let read = async {
			let client = services.bulletin.client("bitswap_v1_get").await.map_err(|error| {
				SourceError::Transient(format!("connecting to Bulletin: {error}"))
			})?;
			bitswap_get(&client, cid).await.map_err(|error| match error {
				BitswapError::Unanswerable(reason) | BitswapError::MethodMissing(reason) => {
					SourceError::Permanent(reason)
				},
				BitswapError::Connection(reason) | BitswapError::Failed(reason) => {
					SourceError::Transient(reason)
				},
			})
		};
		pin_mut!(read);
		match future::select(read, RealTimer.sleep(RPC_TIMEOUT)).await {
			Either::Left((answer, _)) => answer,
			Either::Right(_) => Err(SourceError::Transient("bitswap_v1_get timed out".to_string())),
		}
	}
}

/// Lookups over one blob source: the CID for each key, and the check that what comes back
/// hashes to it.
pub struct BulletinLookup<S> {
	source: Arc<S>,
	timer: Arc<dyn Timer>,
	poll_interval: Duration,
}

impl<S> Clone for BulletinLookup<S> {
	fn clone(&self) -> Self {
		Self {
			source: Arc::clone(&self.source),
			timer: Arc::clone(&self.timer),
			poll_interval: self.poll_interval,
		}
	}
}

impl<S: BlobSource + 'static> BulletinLookup<S> {
	/// Lookups over `source` that ask again about every Bulletin block after a miss.
	pub fn new(source: Arc<S>, timer: Arc<dyn Timer>) -> Self {
		Self { source, timer, poll_interval: POLL_INTERVAL }
	}

	/// Asks again every poll interval until the source holds the blob, or fails once it never
	/// can.
	async fn until_held(self, cid: BulletinCid) -> Result<Vec<u8>, api::GenericError> {
		loop {
			self.timer.sleep(self.poll_interval).await;
			match self.read(&cid).await {
				Ok(Some(value)) => return Ok(value),
				Ok(None) => {},
				Err(SourceError::Transient(reason)) => {
					debug!(%cid, %reason, "preimage lookup failed, still trying")
				},
				Err(SourceError::Permanent(reason)) => return Err(api::GenericError { reason }),
			}
		}
	}

	/// One read: the blob if the source holds it and it hashes to `cid`.
	async fn read(&self, cid: &BulletinCid) -> Result<Option<Vec<u8>>, SourceError> {
		let Some(value) = self.source.get(cid).await? else {
			return Ok(None);
		};
		if !cid.matches(&value) {
			warn!(%cid, "preimage source returned bytes that do not hash to the CID");
			return Ok(None);
		}
		info!(%cid, size = value.len(), "preimage read from Bulletin");
		Ok(Some(value))
	}
}

/// The current answer at once, then the value once it can be read, then the end.
impl<S: BlobSource + 'static> PreimageHost for BulletinLookup<S> {
	fn lookup_preimage(
		&self,
		key: [u8; 32],
	) -> BoxStream<'static, Result<Option<Vec<u8>>, api::GenericError>> {
		let lookup = self.clone();
		async move {
			let cid = BulletinCid::from(key);
			match lookup.read(&cid).await {
				Ok(Some(value)) => return stream::once(future::ready(Ok(Some(value)))).boxed(),
				Ok(None) => {},
				Err(SourceError::Transient(reason)) => {
					warn!(%cid, %reason, "preimage lookup failed, still trying")
				},
				Err(SourceError::Permanent(reason)) => {
					return stream::iter([Ok(None), Err(api::GenericError { reason })]).boxed();
				},
			}
			let held = lookup.until_held(cid).map(|held| held.map(Some));
			stream::once(future::ready(Ok(None))).chain(stream::once(held)).boxed()
		}
		.flatten_stream()
		.boxed()
	}
}

#[cfg(test)]
mod tests {
	use std::{collections::VecDeque, sync::Mutex};

	use super::*;
	use crate::host_internal::bulletin::preimage_key;

	/// A source that answers from a script, one entry per call, and counts the calls.
	struct Scripted {
		answers: Mutex<VecDeque<Result<Option<Vec<u8>>, SourceError>>>,
		calls: Mutex<u32>,
	}

	#[async_trait::async_trait]
	impl BlobSource for Scripted {
		async fn get(&self, _cid: &BulletinCid) -> Result<Option<Vec<u8>>, SourceError> {
			*self.calls.lock().unwrap() += 1;
			self.answers
				.lock()
				.unwrap()
				.pop_front()
				.expect("scripted source asked more often than scripted")
		}
	}

	fn blob() -> Vec<u8> {
		b"bulletin lookup test blob".to_vec()
	}

	fn scripted(answers: Vec<Result<Option<Vec<u8>>, SourceError>>) -> BulletinLookup<Scripted> {
		let source = Scripted { answers: Mutex::new(answers.into()), calls: Mutex::new(0) };
		BulletinLookup {
			source: Arc::new(source),
			timer: Arc::new(RealTimer),
			poll_interval: Duration::from_millis(1),
		}
	}

	/// Every item of the subscription, errors by their reason.
	async fn events(
		lookup: &BulletinLookup<Scripted>,
		key: &[u8; 32],
	) -> Vec<Result<Option<Vec<u8>>, String>> {
		lookup
			.lookup_preimage(*key)
			.map(|item| item.map_err(|error| error.reason))
			.collect()
			.await
	}

	fn calls(lookup: &BulletinLookup<Scripted>) -> u32 {
		*lookup.source.calls.lock().unwrap()
	}

	fn never() -> Result<Option<Vec<u8>>, SourceError> {
		Err(SourceError::Permanent("invalid CID".to_string()))
	}

	#[tokio::test]
	async fn a_held_blob_is_emitted_at_once_and_the_subscription_ends() {
		let lookup = scripted(vec![Ok(Some(blob()))]);
		assert_eq!(events(&lookup, &preimage_key(&blob())).await, vec![Ok(Some(blob()))]);
	}

	#[tokio::test]
	async fn a_miss_is_reported_at_once_then_the_blob_when_it_lands_then_the_end() {
		// The caller hears the miss immediately, so it can show "waiting", and is not asked
		// to resubscribe: the same subscription delivers the blob once another host's
		// submission has landed.
		let lookup = scripted(vec![Ok(None), Ok(None), Ok(Some(blob()))]);
		assert_eq!(events(&lookup, &preimage_key(&blob())).await, vec![Ok(None), Ok(Some(blob()))]);
		assert_eq!(calls(&lookup), 3);
	}

	#[tokio::test]
	async fn bytes_that_do_not_hash_to_the_key_are_never_emitted() {
		let lookup = scripted(vec![Ok(Some(b"forged".to_vec())), Ok(Some(blob()))]);
		assert_eq!(events(&lookup, &preimage_key(&blob())).await, vec![Ok(None), Ok(Some(blob()))]);
	}

	#[tokio::test]
	async fn a_failing_source_is_a_miss_not_the_end_of_the_subscription() {
		let transient = Err(SourceError::Transient("node down".to_string()));
		let lookup = scripted(vec![transient, Ok(Some(blob()))]);
		assert_eq!(events(&lookup, &preimage_key(&blob())).await, vec![Ok(None), Ok(Some(blob()))]);
	}

	/// The caller must learn that the host can never look this up, not see an ordinary end.
	#[tokio::test]
	async fn a_request_the_node_can_never_answer_ends_the_subscription_with_an_error() {
		let lookup = scripted(vec![never()]);
		assert_eq!(
			events(&lookup, &preimage_key(&blob())).await,
			vec![Ok(None), Err("invalid CID".to_string())]
		);
	}

	#[tokio::test]
	async fn a_request_that_turns_permanent_while_polling_stops_the_poll() {
		let lookup = scripted(vec![Ok(None), never()]);
		assert_eq!(
			events(&lookup, &preimage_key(&blob())).await,
			vec![Ok(None), Err("invalid CID".to_string())]
		);
		assert_eq!(calls(&lookup), 2);
	}
}
