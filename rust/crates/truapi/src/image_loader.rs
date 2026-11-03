//! Images for hosts to draw: [`ImageLoader`] reads the bytes an [`ImageReference`] names from
//! Bulletin, checks them against their CID and the policy, and hands them over.

mod policy;

pub use policy::MAX_IMAGE_BYTES;

use core::{future::Future, ops::ControlFlow, time::Duration};
use std::sync::Arc;

use futures::FutureExt;
use parity_scale_codec::{Decode, Encode};
use tracing::{debug, instrument, warn};

use crate::{
	host_internal::bulletin::{BitswapError, BulletinCid, bitswap_get},
	public_profile::{PublicProfileError, PublicProfiles},
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum ImageFormat {
	/// PNG.
	#[codec(index = 0)]
	Png,
	/// JPEG.
	#[codec(index = 1)]
	Jpeg,
	/// GIF.
	#[codec(index = 3)]
	Gif,
	/// WebP.
	#[codec(index = 2)]
	Webp,
}

impl ImageFormat {
	/// Validates the format, file size and dimensions against the core's image policy.
	pub fn validate(bytes: &[u8], declared_format: Option<Self>) -> Result<Self, ImageError> {
		policy::check(bytes, declared_format)
			.map_err(|rejection| ImageError::Rejected { reason: rejection.to_string() })
	}
}

// Products write the CIDs in their renderer trees, so a bad one is input, not a bug. UniFFI hands a
// failed lift to the host as the call's own error only when the failure is that error.
#[cfg(not(target_arch = "wasm32"))]
uniffi::custom_type!(BulletinCid, String, {
	lower: |cid| cid.to_string(),
	try_lift: |cid| {
		Ok(cid.parse::<BulletinCid>().map_err(|rejection| ImageError::Unsupported {
			reason: rejection.to_string(),
		})?)
	},
});

/// Where an image comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum ImageReference {
	/// A Bulletin blob.
	Bulletin {
		/// The blob's CID.
		cid: BulletinCid,
		/// The format the source declares, such as a manifest icon's. Bytes in another format
		/// are rejected.
		declared_format: Option<ImageFormat>,
	},
	/// The avatar in a dotNS public profile.
	Username {
		/// A dotNS username, such as `alice` or `alice.01`.
		username: String,
		/// Read the profile from the chain instead of the native record cache. Browsers
		/// always read the chain.
		force: bool,
	},
}

/// An image that passed every check.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct LoadedImage {
	/// The image file.
	pub bytes: Vec<u8>,
	/// The format read from the bytes.
	pub format: ImageFormat,
	/// Names these bytes and nothing else, so a host can key its disk cache on it: the CID.
	pub content_key: BulletinCid,
}

/// Why an image did not load. A host draws its placeholder in every case.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Error))]
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
	/// The profile could not name an image, or Bulletin did not return its bytes.
	#[error("image unavailable: {reason}")]
	Unavailable {
		/// What failed.
		reason: String,
	},
}

/// Reads the images hosts draw from Bulletin and checks them against their CID and the policy.
pub struct ImageLoader {
	services: Arc<RuntimeServices>,
	timer: Arc<dyn Timer>,
}

impl ImageLoader {
	/// A loader reading over the Bulletin connection of `services`, pausing between attempts and
	/// timing out on `timer`.
	pub fn new(services: Arc<RuntimeServices>, timer: Arc<dyn Timer>) -> Self {
		Self { services, timer }
	}

	/// The image `reference` names. Dropping the future stops the load. A username without an
	/// avatar is immediately unavailable; a later call can refresh its record with `force`.
	#[instrument(skip_all, fields(runtime.method = "image_loader.load"))]
	pub async fn load(&self, reference: ImageReference) -> Result<LoadedImage, ImageError> {
		let (cid, declared_format) = match reference {
			ImageReference::Bulletin { cid, declared_format } => (cid, declared_format),
			ImageReference::Username { username, force } => {
				let record = PublicProfiles::new(self.services.clone())
					.resolve(&username, force)
					.await
					.map_err(|error| match error {
						PublicProfileError::InvalidUsername => {
							ImageError::Unsupported { reason: error.to_string() }
						},
						_ => ImageError::Unavailable { reason: error.to_string() },
					})?;
				let avatar = record.avatar().ok_or_else(|| ImageError::Unavailable {
					reason: "the public profile names no avatar".to_string(),
				})?;
				(avatar.cid, Some(avatar.format))
			},
		};
		let bytes = self.read_verified(&cid, || self.read(&cid)).await?;
		let format = ImageFormat::validate(&bytes, declared_format)?;
		Ok(LoadedImage { bytes, format, content_key: cid })
	}

	async fn read(&self, cid: &BulletinCid) -> Result<Option<Vec<u8>>, BitswapError> {
		let client = self
			.services
			.bulletin
			.client("image_loader.load")
			.await
			.map_err(|failure| BitswapError::Connection(failure.reason()))?;
		bitswap_get(&client, cid).await
	}

	/// Reads `cid` until the bytes hash to it, Bulletin refuses for good, or [`LOAD_TIMEOUT`]
	/// passes.
	async fn read_verified<Read, ReadFuture>(
		&self,
		cid: &BulletinCid,
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
					Ok(Some(bytes)) if cid.matches(&bytes) => ControlFlow::Break(Ok(bytes)),
					Ok(Some(_)) => {
						warn!(%cid, "Bulletin returned bytes that do not hash to the CID");
						ControlFlow::Continue(MISS_PAUSE)
					},
					Ok(None) => {
						debug!(%cid, "Bulletin does not hold the image yet");
						ControlFlow::Continue(MISS_PAUSE)
					},
					Err(
						BitswapError::Unanswerable(reason) | BitswapError::MethodMissing(reason),
					) => ControlFlow::Break(Err(ImageError::Unavailable { reason })),
					Err(error) => {
						debug!(%cid, %error, "Bulletin read failed, asking again");
						ControlFlow::Continue(FAILURE_PAUSE)
					},
				}
			}
		};
		let timer = &*self.timer;
		futures::select_biased! {
			result = retry_on(timer, attempt, |pause| pause).fuse() => result,
			() = timer.sleep(LOAD_TIMEOUT).fuse() => Err(ImageError::Unavailable {
				reason: format!("no verified bytes within {} s", LOAD_TIMEOUT.as_secs()),
			}),
		}
	}
}

#[cfg(test)]
mod tests;
