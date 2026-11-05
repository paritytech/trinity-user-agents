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
	timer.advance(FAILURE_PAUSE);
	assert_eq!(futures::poll!(&mut load), Poll::Pending, "a miss waits for the next block");
	timer.advance(MISS_PAUSE - FAILURE_PAUSE);
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
	assert_eq!(
		futures::poll!(&mut load),
		Poll::Ready(Err(ImageError::Unavailable {
			reason: "no verified bytes within 30 s".into()
		}))
	);
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
		rpc_method_responses: vec![("bitswap_v1_get", format!("\"0x{}\"", hex::encode(image())))],
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
	let cid = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyku";

	let loaded = load_image(
		&services,
		&ManualTimer::default(),
		ImageReference::Bulletin { cid: cid.into(), declared_format: None },
	)
	.await;
	assert_eq!(
		(loaded, platform.chain_connects.lock().unwrap().clone()),
		(
			Err(ImageError::Unsupported {
				reason: format!("{cid} is not a raw blake2b-256 Bulletin CID")
			}),
			vec![]
		)
	);
}
