use parity_scale_codec::Encode;
use subxt::utils::WrapperKeepOpaque;

use super::*;
use crate::{
	public_profile::{ProfileAvatar, ProfileEntry, ProfileRecord},
	unix_time::current_unix_secs,
};

fn profile(cid: BulletinCid, format: ImageFormat) -> ProfileRecord {
	ProfileRecord {
		entries: vec![WrapperKeepOpaque::from_value(ProfileEntry::Avatar(ProfileAvatar {
			cid,
			format,
		}))],
	}
}

async fn cache_profile(loader: &ImageLoader, record: ProfileRecord) -> tempfile::TempDir {
	let (directory, db) = crate::store::open_temporary_core_db();
	db.write(move |tx| {
		tx.execute(
			"INSERT INTO public_profiles (username, public_record, expires_at) VALUES (?1, ?2, ?3)",
			rusqlite::params!["alice.01", record.encode(), current_unix_secs() + 3600],
		)?;
		Ok(())
	})
	.await
	.unwrap();
	assert!(loader.services.install_core_db(db));
	directory
}

fn username(force: bool) -> ImageReference {
	ImageReference::Username { username: "alice.01".to_string(), force }
}

#[tokio::test]
async fn username_images_use_the_verified_bulletin_loader() {
	let cid = image_cid();
	let platform = Arc::new(StubPlatform {
		rpc_method_responses: vec![("bitswap_v1_get", format!("\"0x{}\"", hex::encode(image())))],
		preimage_lookup_value: Some(b"host preimages are not used".to_vec()),
		..Default::default()
	});
	let loader = image_loader(platform.clone(), Arc::default());
	let _directory = cache_profile(&loader, profile(cid.clone(), ImageFormat::Png)).await;

	assert_eq!(
		loader.load(username(false)).await,
		Ok(LoadedImage { bytes: image(), format: ImageFormat::Png, content_key: cid })
	);
	assert_eq!(*platform.chain_connects.lock().unwrap(), vec![BULLETIN_GENESIS]);
}

#[tokio::test]
async fn an_absent_avatar_finishes_without_bulletin() {
	let platform = Arc::new(StubPlatform::default());
	let loader = image_loader(platform.clone(), Arc::default());
	let _directory = cache_profile(&loader, ProfileRecord::default()).await;

	assert_eq!(
		loader.load(username(false)).await,
		Err(ImageError::Unavailable { reason: "the public profile names no avatar".into() })
	);
	assert!(platform.chain_connects.lock().unwrap().is_empty());
}

#[tokio::test]
async fn forcing_a_username_reads_the_chain_despite_a_cached_record() {
	let platform =
		Arc::new(StubPlatform { chain_connect_error: Some("offline"), ..Default::default() });
	let loader = image_loader(platform.clone(), Arc::default());
	let _directory = cache_profile(&loader, ProfileRecord::default()).await;

	assert_eq!(
		loader.load(username(true)).await,
		Err(ImageError::Unavailable {
			reason: "dotNS lookup failed: Asset Hub follow stopped before initialization".into(),
		})
	);
	assert_eq!(*platform.chain_connects.lock().unwrap(), vec![[3; 32]]);
}

#[tokio::test]
async fn an_opaque_avatar_with_an_invalid_cid_never_reaches_bulletin() {
	let platform = Arc::new(StubPlatform::default());
	let loader = image_loader(platform.clone(), Arc::default());
	let record = ProfileRecord {
		entries: vec![WrapperKeepOpaque::from_encoded(
			[vec![0u8], "not a CID".encode(), vec![0u8]].concat(),
		)],
	};
	let _directory = cache_profile(&loader, record).await;

	assert_eq!(
		loader.load(username(false)).await,
		Err(ImageError::Unavailable { reason: "the public profile names no avatar".into() })
	);
	assert!(platform.chain_connects.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_profile_cannot_misdeclare_the_images_format() {
	let platform = Arc::new(StubPlatform {
		rpc_method_responses: vec![("bitswap_v1_get", format!("\"0x{}\"", hex::encode(image())))],
		..Default::default()
	});
	let loader = image_loader(platform, Arc::default());
	let _directory = cache_profile(&loader, profile(image_cid(), ImageFormat::Jpeg)).await;

	assert_eq!(
		loader.load(username(false)).await,
		Err(ImageError::Rejected { reason: "declared Jpeg, but the bytes are Png".into() })
	);
}

#[tokio::test]
async fn an_invalid_username_is_unsupported_without_reading_storage() {
	let loader = image_loader(Arc::default(), Arc::default());

	assert_eq!(
		loader
			.load(ImageReference::Username { username: "alice.1".to_string(), force: false })
			.await,
		Err(ImageError::Unsupported { reason: "not a dotNS username".into() })
	);
}
