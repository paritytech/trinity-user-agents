use core::{future::ready, task::Poll};
use std::{
	collections::VecDeque,
	sync::{Arc, Mutex},
};

use super::*;
use crate::{
	host_internal::bulletin::preimage_key,
	platform::HostInfo,
	test_support::{StubPlatform, test_spawner},
	time::ManualTimer,
};

const BULLETIN_GENESIS: [u8; 32] = [2; 32];

fn image() -> Vec<u8> {
	[b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".as_slice(), &16u32.to_be_bytes(), &16u32.to_be_bytes()]
		.concat()
}

fn image_cid() -> BulletinCid {
	BulletinCid::from(preimage_key(&image()))
}

/// Answers each read from `answers`, then as a node that does not hold the blob.
fn node(
	answers: Vec<Result<Option<Vec<u8>>, BitswapError>>,
) -> impl Fn() -> core::future::Ready<Result<Option<Vec<u8>>, BitswapError>> {
	let answers = Mutex::new(VecDeque::from(answers));
	move || ready(answers.lock().unwrap().pop_front().unwrap_or(Ok(None)))
}

fn image_loader(platform: Arc<StubPlatform>, timer: Arc<ManualTimer>) -> ImageLoader {
	let services = RuntimeServices::new(
		platform,
		HostInfo {
			name: "Image test".into(),
			icon: None,
			version: None,
			platform: truapi::latest::HostPlatform::Unknown,
		},
		crate::test_support::test_stack("paseo", [1; 32], BULLETIN_GENESIS, Some([3; 32])),
		test_spawner(),
	);
	ImageLoader::new(services, timer)
}

#[tokio::test]
async fn a_blob_is_read_once_bulletin_holds_it() {
	let timer = Arc::new(ManualTimer::default());
	let loader = image_loader(Arc::default(), timer.clone());
	let cid = image_cid();
	let read = node(vec![Ok(None), Ok(Some(image()))]);
	let mut load = core::pin::pin!(loader.read_verified(&cid, read));

	assert_eq!(futures::poll!(&mut load), Poll::Pending);
	timer.advance(FAILURE_PAUSE);
	assert_eq!(futures::poll!(&mut load), Poll::Pending, "a miss waits for the next block");
	timer.advance(MISS_PAUSE - FAILURE_PAUSE);
	assert_eq!(futures::poll!(&mut load), Poll::Ready(Ok(image())));
}

/// A light client answers with an error until its first Bitswap peer connects, about half a
/// second after start, so images drawn at launch must not wait a whole block.
#[tokio::test]
async fn a_failed_request_is_retried_sooner_than_a_miss() {
	let timer = Arc::new(ManualTimer::default());
	let loader = image_loader(Arc::default(), timer.clone());
	let cid = image_cid();
	let no_peers = BitswapError::Failed("No Bitswap peers connected (-32812)".into());
	let read = node(vec![Err(no_peers), Ok(Some(image()))]);
	let mut load = core::pin::pin!(loader.read_verified(&cid, read));

	assert_eq!(futures::poll!(&mut load), Poll::Pending);
	timer.advance(FAILURE_PAUSE);
	assert_eq!(futures::poll!(&mut load), Poll::Ready(Ok(image())));
}

#[tokio::test]
async fn bytes_that_do_not_hash_to_the_cid_are_never_returned() {
	let timer = Arc::new(ManualTimer::default());
	let loader = image_loader(Arc::default(), timer.clone());
	let cid = image_cid();
	let read = node(vec![Ok(Some(b"forged".to_vec())), Ok(Some(image()))]);
	let mut load = core::pin::pin!(loader.read_verified(&cid, read));

	assert_eq!(futures::poll!(&mut load), Poll::Pending);
	timer.advance(MISS_PAUSE);
	assert_eq!(futures::poll!(&mut load), Poll::Ready(Ok(image())));
}

#[tokio::test]
async fn a_request_bulletin_can_never_answer_ends_the_load() {
	let timer = Arc::new(ManualTimer::default());
	let loader = image_loader(Arc::default(), timer.clone());
	let cid = image_cid();
	let read = node(vec![Err(BitswapError::Unanswerable("invalid CID".into()))]);

	assert_eq!(
		loader.read_verified(&cid, read).await,
		Err(ImageError::Unavailable { reason: "invalid CID".into() })
	);
}

#[tokio::test]
async fn a_load_ends_at_its_timeout() {
	let timer = Arc::new(ManualTimer::default());
	let loader = image_loader(Arc::default(), timer.clone());
	let cid = image_cid();
	let mut load = core::pin::pin!(loader.read_verified(&cid, node(vec![])));

	assert_eq!(futures::poll!(&mut load), Poll::Pending);
	timer.advance(LOAD_TIMEOUT - Duration::from_secs(1));
	assert_eq!(futures::poll!(&mut load), Poll::Pending);
	timer.advance(Duration::from_secs(1));
	assert_eq!(
		futures::poll!(&mut load),
		Poll::Ready(Err(ImageError::Unavailable {
			reason: "no verified bytes within 30 s".into()
		}))
	);
}

#[tokio::test]
async fn an_image_is_read_over_the_cores_bulletin_connection() {
	let cid = image_cid();
	let platform = Arc::new(StubPlatform {
		rpc_method_responses: vec![("bitswap_v1_get", format!("\"0x{}\"", hex::encode(image())))],
		..Default::default()
	});

	let loaded = image_loader(platform.clone(), Arc::default())
		.load(ImageReference::Bulletin {
			cid: cid.clone(),
			declared_format: Some(ImageFormat::Png),
		})
		.await;
	assert_eq!(
		(loaded, platform.chain_connects.lock().unwrap().clone()),
		(
			Ok(LoadedImage { bytes: image(), format: ImageFormat::Png, content_key: cid }),
			vec![BULLETIN_GENESIS]
		)
	);
}

/// Renderer trees carry CIDs products write, so a bad one is input, not a bug. It must reach the
/// host as the `Unsupported` error `loadImage` declares, which UniFFI returns only when the
/// failed lift is an `ImageError`.
#[test]
fn a_cid_a_host_passes_is_checked_as_it_enters_the_core() {
	let lift = |cid: String| {
		<BulletinCid as uniffi::Lift<crate::UniFfiTag>>::try_lift(<String as uniffi::Lower<
			crate::UniFfiTag,
		>>::lower(cid))
		.map_err(|error| error.downcast::<ImageError>().ok())
	};
	let sha2_cid = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyku";

	assert_eq!(
		[lift(image_cid().to_string()), lift(sha2_cid.to_string())],
		[
			Ok(image_cid()),
			Err(Some(ImageError::Unsupported {
				reason: "the CID's hash is not blake2b-256".into()
			})),
		]
	);
}
