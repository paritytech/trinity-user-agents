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
					debug!(cid, %error, "Bulletin read failed, asking again");
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
mod tests;
