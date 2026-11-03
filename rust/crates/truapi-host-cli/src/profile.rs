//! Reading and publishing the active identity's public dotNS profile.

mod chain;

use std::{
	fmt,
	fs::File,
	io::{Cursor, Read},
	path::Path,
};

use anyhow::{Context, Result, ensure};
use async_trait::async_trait;
use image::ImageReader;
use truapi::{
	BulletinCid, ImageFormat, MAX_IMAGE_BYTES, ProfileEntry, ProfileRecord, ProfileRecordLocation,
	PublicProfileError, SigningHostRuntime, host_logic::dotns_gateway::is_dotted_lite_username,
	platform::is_dns_label, preimage_key,
};

use crate::{dotns_read::AssetHubReader, network::NetworkConfig};

/// Reads the current on-chain record without using a cached copy.
pub async fn read(network: &NetworkConfig, username: &str) -> Result<String> {
	ensure!(
		is_dns_label(username) || is_dotted_lite_username(username),
		PublicProfileError::InvalidUsername
	);
	let mut reader = AssetHubReader::connect(network).await?;
	let text = match ProfileRecordLocation::resolve(&mut reader, username).await? {
		Some(location) => location.read(&mut reader).await?,
		None => None,
	};
	let record = text.as_deref().and_then(ProfileRecord::from_text).unwrap_or_default();
	Ok(format!("{username}\n{}", format_record(&record)))
}

/// Publishes a local avatar for the identity that owns `username`.
pub async fn upload(
	runtime: &SigningHostRuntime,
	network: NetworkConfig,
	entropy: &[u8],
	username: &str,
	path: &Path,
) -> Result<String> {
	let path = path.to_owned();
	let avatar = tokio::task::spawn_blocking(move || read_image(&path))
		.await
		.context("join profile image reader")??;
	let record = chain::publish(runtime, network, entropy, username, avatar).await?;
	Ok(format!("Profile published for {username}\n{}", format_record(&record)))
}

fn format_record(record: &ProfileRecord) -> String {
	let entries = fmt::from_fn(|formatter| {
		formatter
			.debug_list()
			.entries(record.entries.iter().map(|entry| {
				fmt::from_fn(move |formatter| match entry.try_decode() {
					Some(ProfileEntry::Avatar(avatar)) => {
						let value = fmt::from_fn(|formatter| {
							formatter
								.debug_struct("ProfileAvatar")
								.field("cid", &avatar.cid.to_string())
								.field("format", &avatar.format)
								.finish()
						});
						let entry = fmt::from_fn(|formatter| {
							formatter.debug_tuple("Avatar").field(&value).finish()
						});
						formatter.debug_tuple("Known").field(&entry).finish()
					},
					None => formatter.debug_tuple("Opaque").field(&entry.encoded()).finish(),
				})
			}))
			.finish()
	});
	let record = fmt::from_fn(|formatter| {
		formatter.debug_struct("ProfileRecord").field("entries", &entries).finish()
	});
	format!("{record:#?}")
}

fn read_image(path: &Path) -> Result<(ImageFormat, Vec<u8>)> {
	let mut bytes = Vec::new();
	File::open(path)
		.with_context(|| format!("open avatar {}", path.display()))?
		.take(MAX_IMAGE_BYTES as u64 + 1)
		.read_to_end(&mut bytes)
		.with_context(|| format!("read avatar {}", path.display()))?;
	ensure!(bytes.len() <= MAX_IMAGE_BYTES, "avatar exceeds the 2 MiB limit");
	let format = ImageFormat::validate(&bytes, None)?;
	ImageReader::new(Cursor::new(&bytes))
		.with_guessed_format()
		.context("avatar is not a supported image")?
		.decode()
		.context("avatar image cannot be decoded")?;
	Ok((format, bytes))
}

#[async_trait]
trait ProfileBackend: Send {
	type Prepared: Send;

	async fn read(&mut self) -> Result<Option<String>>;
	async fn prepare(&mut self, record: &ProfileRecord) -> Result<Self::Prepared>;
	async fn upload(&mut self, bytes: Vec<u8>) -> Result<[u8; 32]>;
	async fn publish(&mut self, prepared: Self::Prepared, record: &ProfileRecord) -> Result<()>;
}

struct ProfilePublisher<Backend> {
	backend: Backend,
}

impl<Backend: ProfileBackend> ProfilePublisher<Backend> {
	async fn publish(&mut self, (format, bytes): (ImageFormat, Vec<u8>)) -> Result<ProfileRecord> {
		let original = self.backend.read().await?;
		let mut record = match original.as_deref() {
			Some(text) => ProfileRecord::from_text(text)
				.context("existing profile record is malformed; refusing to overwrite it")?,
			None => ProfileRecord::default(),
		};
		let key = preimage_key(&bytes);
		record.set_avatar(BulletinCid::from(key), format);
		let prepared = self.backend.prepare(&record).await?;
		ensure!(
			self.backend.upload(bytes).await? == key,
			"uploaded avatar key does not match its bytes"
		);
		ensure!(
			self.backend.read().await? == original,
			"profile changed during the upload; retry to preserve the new record"
		);
		self.backend.publish(prepared, &record).await?;
		Ok(record)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::{collections::VecDeque, fs};
	use subxt::utils::WrapperKeepOpaque;
	use truapi::ProfileAvatar;

	#[test]
	fn profile_output_decodes_known_entries_and_keeps_opaque_entries_in_order() {
		let record = ProfileRecord {
			entries: vec![
				WrapperKeepOpaque::from_encoded(vec![7, 1, 2]),
				WrapperKeepOpaque::from_value(ProfileEntry::Avatar(ProfileAvatar {
					cid: "bafk2bzaceaf5do2x4p46vaazk2txb7qqnd4vaobvi6wkixq5hu2u2v6qctfqy"
						.parse()
						.unwrap(),
					format: ImageFormat::Webp,
				})),
			],
		};

		assert_eq!(
			format_record(&record),
			r#"ProfileRecord {
    entries: [
        Opaque(
            [
                7,
                1,
                2,
            ],
        ),
        Known(
            Avatar(
                ProfileAvatar {
                    cid: "bafk2bzaceaf5do2x4p46vaazk2txb7qqnd4vaobvi6wkixq5hu2u2v6qctfqy",
                    format: Webp,
                },
            ),
        ),
    ],
}"#
		);
	}

	#[derive(Debug, PartialEq, Eq)]
	enum Operation {
		Read,
		Prepare(ProfileRecord),
		Upload(Vec<u8>),
		Publish(ProfileRecord),
	}

	#[derive(Default)]
	struct Publisher {
		records: VecDeque<Option<String>>,
		failure: Option<&'static str>,
		operations: Vec<Operation>,
	}

	#[async_trait]
	impl ProfileBackend for Publisher {
		type Prepared = ProfileRecord;

		async fn read(&mut self) -> Result<Option<String>> {
			self.operations.push(Operation::Read);
			Ok(self.records.pop_front().flatten())
		}

		async fn prepare(&mut self, record: &ProfileRecord) -> Result<Self::Prepared> {
			self.operations.push(Operation::Prepare(record.clone()));
			ensure!(self.failure != Some("prepare"), "record cannot be published");
			Ok(record.clone())
		}

		async fn upload(&mut self, bytes: Vec<u8>) -> Result<[u8; 32]> {
			let key = preimage_key(&bytes);
			self.operations.push(Operation::Upload(bytes));
			ensure!(self.failure != Some("upload"), "Bulletin upload failed");
			Ok(key)
		}

		async fn publish(
			&mut self,
			prepared: Self::Prepared,
			record: &ProfileRecord,
		) -> Result<()> {
			assert_eq!(prepared, *record);
			self.operations.push(Operation::Publish(prepared));
			ensure!(self.failure != Some("publish"), "profile publication failed");
			Ok(())
		}
	}

	fn avatar() -> (ImageFormat, Vec<u8>) {
		(ImageFormat::Png, vec![1, 2, 3])
	}

	fn updated_record() -> ProfileRecord {
		let (format, bytes) = avatar();
		let mut record = ProfileRecord::default();
		record.set_avatar(BulletinCid::from(preimage_key(&bytes)), format);
		record
	}

	#[tokio::test]
	async fn the_record_is_published_only_after_the_image_is_stored() {
		let record = updated_record();
		let mut publisher = ProfilePublisher { backend: Publisher::default() };

		let result = publisher.publish(avatar()).await.unwrap();

		assert_eq!(
			(result, publisher.backend.operations),
			(
				record.clone(),
				vec![
					Operation::Read,
					Operation::Prepare(record.clone()),
					Operation::Upload(avatar().1),
					Operation::Read,
					Operation::Publish(record),
				]
			)
		);
	}

	#[tokio::test]
	async fn a_malformed_record_is_not_overwritten_or_uploaded_over() {
		let mut publisher = ProfilePublisher {
			backend: Publisher {
				records: [Some("not base64".to_string())].into(),
				..Default::default()
			},
		};

		let error = publisher.publish(avatar()).await.unwrap_err();

		assert_eq!(
			(error.to_string(), publisher.backend.operations),
			(
				"existing profile record is malformed; refusing to overwrite it".to_string(),
				vec![Operation::Read]
			)
		);
	}

	#[tokio::test]
	async fn a_failed_preflight_does_not_upload_an_unpublishable_avatar() {
		let mut publisher = ProfilePublisher {
			backend: Publisher { failure: Some("prepare"), ..Default::default() },
		};

		let error = publisher.publish(avatar()).await.unwrap_err();

		assert_eq!(
			(error.to_string(), publisher.backend.operations),
			(
				"record cannot be published".to_string(),
				vec![Operation::Read, Operation::Prepare(updated_record())]
			)
		);
	}

	#[tokio::test]
	async fn a_failed_upload_does_not_publish_a_dangling_avatar() {
		let mut publisher = ProfilePublisher {
			backend: Publisher { failure: Some("upload"), ..Default::default() },
		};

		let error = publisher.publish(avatar()).await.unwrap_err();

		assert_eq!(
			(error.to_string(), publisher.backend.operations),
			(
				"Bulletin upload failed".to_string(),
				vec![
					Operation::Read,
					Operation::Prepare(updated_record()),
					Operation::Upload(avatar().1)
				]
			)
		);
	}

	#[tokio::test]
	async fn a_concurrent_profile_change_is_not_overwritten() {
		let mut publisher = ProfilePublisher {
			backend: Publisher {
				records: [None, Some(ProfileRecord::default().to_text().unwrap())].into(),
				..Default::default()
			},
		};

		let error = publisher.publish(avatar()).await.unwrap_err();

		assert_eq!(
			(error.to_string(), publisher.backend.operations),
			(
				"profile changed during the upload; retry to preserve the new record".to_string(),
				vec![
					Operation::Read,
					Operation::Prepare(updated_record()),
					Operation::Upload(avatar().1),
					Operation::Read
				]
			)
		);
	}

	#[tokio::test]
	async fn failed_publication_never_reports_success() {
		let mut publisher = ProfilePublisher {
			backend: Publisher { failure: Some("publish"), ..Default::default() },
		};

		assert_eq!(
			publisher.publish(avatar()).await.unwrap_err().to_string(),
			"profile publication failed"
		);
	}

	#[test]
	fn image_type_is_detected_from_bytes_and_corrupt_files_are_refused() -> Result<()> {
		let directory = tempfile::tempdir()?;
		let path = directory.path().join("avatar.jpeg");
		let mut png = Cursor::new(Vec::new());
		image::DynamicImage::new_rgb8(2, 2).write_to(&mut png, image::ImageFormat::Png)?;
		fs::write(&path, png.get_ref())?;

		assert_eq!(read_image(&path)?, (ImageFormat::Png, png.into_inner()));
		fs::write(&path, b"\x89PNG\r\n\x1a\ntruncated")?;
		assert!(read_image(&path).is_err());
		Ok(())
	}

	#[test]
	fn gif_uploads_use_the_shared_format_and_keep_the_original_bytes() -> Result<()> {
		let directory = tempfile::tempdir()?;
		let path = directory.path().join("avatar.gif");
		let bytes = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff\x21\xf9\x04\x01\x00\x00\x00\x00\x2c\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02\x44\x01\x00\x3b";
		fs::write(&path, bytes)?;

		assert_eq!(read_image(&path)?, (ImageFormat::Gif, bytes.to_vec()));
		Ok(())
	}

	#[test]
	fn upload_images_must_fit_the_image_loaders_dimensions() -> Result<()> {
		let directory = tempfile::tempdir()?;
		let path = directory.path().join("wide.png");
		image::DynamicImage::new_rgb8(5000, 1).save(&path)?;

		assert_eq!(
			read_image(&path).unwrap_err().to_string(),
			"image rejected: 5000x1 pixels is over the 4096-pixel side limit"
		);
		Ok(())
	}

	#[test]
	fn oversized_images_are_refused_before_decoding() -> Result<()> {
		let file = tempfile::NamedTempFile::new()?;
		file.as_file().set_len(MAX_IMAGE_BYTES as u64 + 1)?;

		assert_eq!(
			read_image(file.path()).unwrap_err().to_string(),
			"avatar exceeds the 2 MiB limit"
		);
		Ok(())
	}
}
