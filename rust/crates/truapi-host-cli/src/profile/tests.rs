use std::{collections::VecDeque, fs};

use super::*;

#[test]
fn profile_output_uses_a_cid_that_can_be_loaded() {
	let cid = "bafk2bzaceaf5do2x4p46vaazk2txb7qqnd4vaobvi6wkixq5hu2u2v6qctfqy";
	let profile = PublicProfile {
		avatar: Some(ProfileAvatar { cid: cid.parse().unwrap(), format: ImageFormat::Webp }),
	};
	assert_eq!(format_profile("alice.01", &profile), format!("alice.01\nAvatar: {cid} (Webp)"));
	assert_eq!(format_profile("alice.01", &PublicProfile { avatar: None }), "alice.01\nNo avatar");
}

fn avatar_entry(seed: u8) -> WrapperKeepOpaque<ProfileEntry> {
	WrapperKeepOpaque::from_value(ProfileEntry::Avatar(ProfileAvatar {
		cid: preimage_key(&[seed]).into(),
		format: ImageFormat::Png,
	}))
}

#[test]
fn new_avatar_replaces_the_one_readers_show_and_keeps_other_writers_entries() {
	let unknown = WrapperKeepOpaque::from_encoded(vec![7, 1, 2]);
	let record = ProfileRecord { entries: vec![unknown.clone(), avatar_entry(1), avatar_entry(2)] };

	let updated = with_avatar(
		record,
		ProfileAvatar { cid: preimage_key(&[3]).into(), format: ImageFormat::Png },
	);

	assert_eq!(updated, ProfileRecord { entries: vec![avatar_entry(3), unknown, avatar_entry(2)] });
}

#[derive(Debug, PartialEq)]
enum Operation {
	Read,
	Prepare(String),
	Upload(Vec<u8>),
	Publish(String),
}

#[derive(Default)]
struct Backend {
	records: VecDeque<Option<String>>,
	operations: Vec<Operation>,
}

#[async_trait]
impl ProfileBackend for Backend {
	type Prepared = String;

	async fn read(&mut self) -> Result<Option<String>> {
		self.operations.push(Operation::Read);
		Ok(self.records.pop_front().flatten())
	}

	async fn prepare(&mut self, text: &str) -> Result<String> {
		self.operations.push(Operation::Prepare(text.to_string()));
		Ok(text.to_string())
	}

	async fn upload(&mut self, bytes: Vec<u8>) -> Result<[u8; 32]> {
		let key = preimage_key(&bytes);
		self.operations.push(Operation::Upload(bytes));
		Ok(key)
	}

	async fn publish(&mut self, prepared: String) -> Result<()> {
		self.records.push_back(Some(prepared.clone()));
		self.operations.push(Operation::Publish(prepared));
		Ok(())
	}
}

fn image() -> (ImageFormat, Vec<u8>) {
	(ImageFormat::Png, vec![1, 2, 3])
}

fn published_text() -> String {
	let record = ProfileRecord {
		entries: vec![WrapperKeepOpaque::from_value(ProfileEntry::Avatar(ProfileAvatar {
			cid: preimage_key(&image().1).into(),
			format: ImageFormat::Png,
		}))],
	};
	STANDARD.encode(record.encode())
}

#[tokio::test]
async fn the_record_is_written_only_after_its_avatar_is_stored_and_confirmed_after() {
	let mut backend = Backend { records: [None, None].into(), ..Default::default() };

	let record = publish_avatar(&mut backend, image()).await.unwrap();

	assert_eq!(
		(STANDARD.encode(record.encode()), backend.operations),
		(
			published_text(),
			vec![
				Operation::Read,
				Operation::Prepare(published_text()),
				Operation::Upload(image().1),
				Operation::Read,
				Operation::Publish(published_text()),
				Operation::Read,
			]
		)
	);
}

#[tokio::test]
async fn a_malformed_record_is_never_overwritten() {
	let mut backend =
		Backend { records: [Some("not base64".to_string())].into(), ..Default::default() };

	let error = publish_avatar(&mut backend, image()).await.unwrap_err();

	assert_eq!(
		(error.to_string(), backend.operations),
		(
			"existing profile record is malformed; refusing to overwrite it".to_string(),
			vec![Operation::Read]
		)
	);
}

#[tokio::test]
async fn a_record_changed_during_the_upload_is_not_overwritten() {
	let changed = STANDARD.encode(ProfileRecord { entries: vec![avatar_entry(9)] }.encode());
	let mut backend = Backend { records: [None, Some(changed)].into(), ..Default::default() };

	let error = publish_avatar(&mut backend, image()).await.unwrap_err();

	assert_eq!(
		(error.to_string(), backend.operations),
		(
			"profile changed during the upload; retry to preserve the new record".to_string(),
			vec![
				Operation::Read,
				Operation::Prepare(published_text()),
				Operation::Upload(image().1),
				Operation::Read,
			]
		)
	);
}

#[test]
fn only_images_the_core_would_load_are_uploaded() -> Result<()> {
	let directory = tempfile::tempdir()?;
	let path = directory.path().join("avatar.jpeg");
	let mut png = Cursor::new(Vec::new());
	image::DynamicImage::new_rgb8(2, 2).write_to(&mut png, image::ImageFormat::Png)?;
	fs::write(&path, png.get_ref())?;
	assert_eq!(read_image(&path)?, (ImageFormat::Png, png.into_inner()));

	fs::write(&path, b"\x89PNG\r\n\x1a\ntruncated")?;
	assert!(read_image(&path).is_err());

	image::DynamicImage::new_rgb8(5000, 1).save(directory.path().join("wide.png"))?;
	assert_eq!(
		read_image(&directory.path().join("wide.png")).unwrap_err().to_string(),
		"image rejected: 5000x1 pixels is over the 4096-pixel side limit"
	);
	Ok(())
}
