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
///
/// Missing or malformed records display as empty. Invalid usernames and chain failures
/// return errors.
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
///
/// Rejects unreadable images or images outside the shared policy before chain access.
/// Ownership, allowance and publication failures return errors without reporting success.
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
		.take(u64::try_from(MAX_IMAGE_BYTES)? + 1)
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
mod tests;
