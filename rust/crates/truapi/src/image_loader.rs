//! Images for hosts to draw: [`load_image`] reads the bytes an [`ImageReference`] names from
//! Bulletin, checks them against their CID and the policy, and hands them over.

mod policy;

use core::{future::Future, ops::ControlFlow, time::Duration};

use futures::FutureExt;
use tracing::{debug, instrument, warn};

use crate::{
	host_internal::bulletin::{BitswapError, bitswap_get, preimage_key, preimage_key_from_cid},
	runtime::services::RuntimeServices,
	time::{Timer, retry_on},
};

/// How long a load waits for Bulletin to return verified bytes.
const LOAD_TIMEOUT: Duration = Duration::from_secs(30);
/// Pause before asking again for a blob Bulletin does not hold: about one Bulletin block, in
/// which a new blob can land.
const MISS_PAUSE: Duration = Duration::from_secs(6);
/// Pause after a failed request.
const FAILURE_PAUSE: Duration = Duration::from_secs(1);

/// An image format the core passes on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ImageFormat {
	/// PNG.
	Png,
	/// JPEG.
	Jpeg,
	/// GIF.
	Gif,
	/// WebP.
	Webp,
}

/// Where an image comes from.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum ImageReference {
	/// A Bulletin blob.
	Bulletin {
		/// The blob's CID: CIDv1, the raw codec and a blake2b-256 multihash.
		cid: String,
		/// The format the source declares, such as a manifest icon's. Bytes in another format
		/// are rejected.
		declared_format: Option<ImageFormat>,
	},
}

/// An image that passed every check.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct LoadedImage {
	/// The image file.
	pub bytes: Vec<u8>,
	/// The format read from the bytes.
	pub format: ImageFormat,
	/// Names these bytes and nothing else, so a host can key its disk cache on it: the CID.
	pub content_key: String,
}

/// Why an image did not load. A host draws its placeholder in every case.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum ImageError {
	/// The reference names nothing the core loads, such as a CID that is not a raw
	/// blake2b-256 one.
	#[error("unsupported image reference: {reason}")]
	Unsupported {
		/// What the reference is.
		reason: String,
	},
	/// The bytes are not an image the core passes on.
	#[error("image rejected: {reason}")]
	Rejected {
		/// Which check failed.
		reason: String,
	},
	/// Bulletin did not return the bytes.
	#[error("image unavailable: {reason}")]
	Unavailable {
		/// What failed.
		reason: String,
	},
}

/// The image `reference` names. Dropping the future stops the load.
#[instrument(skip_all, fields(runtime.method = "image_loader.load_image"))]
pub async fn load_image(
	services: &RuntimeServices,
	timer: &dyn Timer,
	reference: ImageReference,
) -> Result<LoadedImage, ImageError> {
	let ImageReference::Bulletin { cid, declared_format } = reference;
	let key = preimage_key_from_cid(&cid).ok_or_else(|| ImageError::Unsupported {
		reason: format!("{cid} is not a raw blake2b-256 Bulletin CID"),
	})?;
	let read = || async {
		let client = services
			.bulletin
			.client("image_loader.load_image")
			.await
			.map_err(|failure| BitswapError::Connection(failure.reason()))?;
		bitswap_get(&client, &cid).await
	};
	let bytes = read_verified(timer, &cid, key, read).await?;
	let format = policy::check(&bytes, declared_format)
		.map_err(|rejection| ImageError::Rejected { reason: rejection.to_string() })?;
	Ok(LoadedImage { bytes, format, content_key: cid })
}

/// Reads `cid` until the bytes hash to `key`, Bulletin refuses for good, or [`LOAD_TIMEOUT`]
/// passes.
async fn read_verified<Read, ReadFuture>(
	timer: &dyn Timer,
	cid: &str,
	key: [u8; 32],
	read: Read,
) -> Result<Vec<u8>, ImageError>
where
	Read: Fn() -> ReadFuture,
	ReadFuture: Future<Output = Result<Option<Vec<u8>>, BitswapError>>,
{
	let attempt = || {
		let read = &read;
		async move {
			match read().await {
				Ok(Some(bytes)) if preimage_key(&bytes) == key => ControlFlow::Break(Ok(bytes)),
				Ok(Some(_)) => {
					warn!(cid, "Bulletin returned bytes that do not hash to the CID");
					ControlFlow::Continue(MISS_PAUSE)
				},
				Ok(None) => {
					debug!(cid, "Bulletin does not hold the image yet");
					ControlFlow::Continue(MISS_PAUSE)
				},
				Err(BitswapError::Unanswerable(reason) | BitswapError::MethodMissing(reason)) => {
					ControlFlow::Break(Err(ImageError::Unavailable { reason }))
				},
				Err(error) => {
					warn!(cid, %error, "Bulletin read failed, asking again");
					ControlFlow::Continue(FAILURE_PAUSE)
				},
			}
		}
	};
	futures::select_biased! {
		result = retry_on(timer, attempt, |pause| pause).fuse() => result,
		() = timer.sleep(LOAD_TIMEOUT).fuse() => Err(ImageError::Unavailable {
			reason: format!("no verified bytes within {} s", LOAD_TIMEOUT.as_secs()),
		}),
	}
}

#[cfg(test)]
mod tests {
	use core::{future::ready, task::Poll};
	use std::{
		collections::VecDeque,
		sync::{Arc, Mutex},
	};

	use super::*;
	use crate::{
		host_internal::bulletin::preimage_cid,
		platform::HostInfo,
		test_support::{StubPlatform, test_spawner},
		time::ManualTimer,
	};

	const BULLETIN_GENESIS: [u8; 32] = [2; 32];

	fn image() -> Vec<u8> {
		[b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".as_slice(), &16u32.to_be_bytes(), &16u32.to_be_bytes()]
			.concat()
	}

	/// Answers each read from `answers`, then as a node that does not hold the blob.
	fn node(
		answers: Vec<Result<Option<Vec<u8>>, BitswapError>>,
	) -> impl Fn() -> core::future::Ready<Result<Option<Vec<u8>>, BitswapError>> {
		let answers = Mutex::new(VecDeque::from(answers));
		move || ready(answers.lock().unwrap().pop_front().unwrap_or(Ok(None)))
	}

	#[tokio::test]
	async fn a_blob_is_read_once_bulletin_holds_it() {
		let timer = ManualTimer::default();
		let read = node(vec![Ok(None), Ok(Some(image()))]);
		let mut load = core::pin::pin!(read_verified(&timer, "cid", preimage_key(&image()), read));

		assert_eq!(futures::poll!(&mut load), Poll::Pending);
		timer.advance(MISS_PAUSE);
		assert_eq!(futures::poll!(&mut load), Poll::Ready(Ok(image())));
	}

	/// A light client answers with an error until its first Bitswap peer connects, about half a
	/// second after start, so images drawn at launch must not wait a whole block.
	#[tokio::test]
	async fn a_failed_request_is_retried_sooner_than_a_miss() {
		let timer = ManualTimer::default();
		let no_peers = BitswapError::Failed("No Bitswap peers connected (-32812)".into());
		let read = node(vec![Err(no_peers), Ok(Some(image()))]);
		let mut load = core::pin::pin!(read_verified(&timer, "cid", preimage_key(&image()), read));

		assert_eq!(futures::poll!(&mut load), Poll::Pending);
		timer.advance(FAILURE_PAUSE);
		assert_eq!(futures::poll!(&mut load), Poll::Ready(Ok(image())));
	}

	#[tokio::test]
	async fn bytes_that_do_not_hash_to_the_cid_are_never_returned() {
		let timer = ManualTimer::default();
		let read = node(vec![Ok(Some(b"forged".to_vec())), Ok(Some(image()))]);
		let mut load = core::pin::pin!(read_verified(&timer, "cid", preimage_key(&image()), read));

		assert_eq!(futures::poll!(&mut load), Poll::Pending);
		timer.advance(MISS_PAUSE);
		assert_eq!(futures::poll!(&mut load), Poll::Ready(Ok(image())));
	}

	#[tokio::test]
	async fn a_request_bulletin_can_never_answer_ends_the_load() {
		let timer = ManualTimer::default();
		let read = node(vec![Err(BitswapError::Unanswerable("invalid CID".into()))]);

		assert_eq!(
			read_verified(&timer, "cid", preimage_key(&image()), read).await,
			Err(ImageError::Unavailable { reason: "invalid CID".into() })
		);
	}

	#[tokio::test]
	async fn a_load_ends_at_its_timeout() {
		let timer = ManualTimer::default();
		let mut load =
			core::pin::pin!(read_verified(&timer, "cid", preimage_key(&image()), node(vec![])));

		assert_eq!(futures::poll!(&mut load), Poll::Pending);
		timer.advance(LOAD_TIMEOUT - Duration::from_secs(1));
		assert_eq!(futures::poll!(&mut load), Poll::Pending);
		timer.advance(Duration::from_secs(1));
		assert!(matches!(
			futures::poll!(&mut load),
			Poll::Ready(Err(ImageError::Unavailable { .. }))
		));
	}

	fn services(platform: StubPlatform) -> (Arc<StubPlatform>, Arc<RuntimeServices>) {
		let platform = Arc::new(platform);
		let services = RuntimeServices::new(
			platform.clone(),
			HostInfo {
				name: "Image test".into(),
				icon: None,
				version: None,
				platform: truapi::latest::HostPlatform::Unknown,
			},
			[1; 32],
			BULLETIN_GENESIS,
			[3; 32],
			test_spawner(),
		);
		(platform, services)
	}

	#[tokio::test]
	async fn an_image_is_read_over_the_cores_bulletin_connection() {
		let cid = preimage_cid(&preimage_key(&image()));
		let (platform, services) = services(StubPlatform {
			rpc_method_responses: vec![(
				"bitswap_v1_get",
				format!("\"0x{}\"", hex::encode(image())),
			)],
			..Default::default()
		});

		let loaded = load_image(
			&services,
			&ManualTimer::default(),
			ImageReference::Bulletin { cid: cid.clone(), declared_format: Some(ImageFormat::Png) },
		)
		.await;
		assert_eq!(
			(loaded, platform.chain_connects.lock().unwrap().clone()),
			(
				Ok(LoadedImage { bytes: image(), format: ImageFormat::Png, content_key: cid }),
				vec![BULLETIN_GENESIS]
			)
		);
	}

	#[tokio::test]
	async fn a_cid_that_is_not_a_bulletin_preimage_is_unsupported() {
		let (platform, services) = services(StubPlatform::default());
		let cid = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyku".to_string();

		let loaded = load_image(
			&services,
			&ManualTimer::default(),
			ImageReference::Bulletin { cid, declared_format: None },
		)
		.await;
		assert!(matches!(loaded, Err(ImageError::Unsupported { .. })));
		assert!(platform.chain_connects.lock().unwrap().is_empty());
	}
}
