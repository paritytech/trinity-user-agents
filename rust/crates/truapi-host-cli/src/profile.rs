//! Public profile reads, and publication of the active identity's avatar through the
//! TrUAPI calls its Humanity product makes.

mod chain;

use std::{
	fs::File,
	io::{Cursor, Read},
	path::Path,
};

use anyhow::{Context, Result, anyhow, ensure};
use async_trait::async_trait;
use image::ImageReader;
use truapi::{
	ImageFormat, MAX_IMAGE_BYTES, ProfileAvatar, ProfileRecord, PublicProfile, SigningHostRuntime,
	check_image, preimage_key,
};

use crate::network::Stack;

/// Reads understood profile metadata from dotNS without provisioning an identity.
///
/// Skips the record cache and reports invalid usernames and lookup failures.
pub async fn read(runtime: &SigningHostRuntime, username: &str) -> Result<String> {
	let profile = runtime.resolve_public_profile(username, true).await?;
	Ok(format_profile(username, &profile))
}

/// Publishes a local avatar for the identity that owns `username`.
///
/// Rejects images the core would not load before any chain access. The allowance and the
/// upload go through the `peopl.<tld>` product's TrUAPI calls, with their usual
/// confirmations. Ownership, upload and publication failures return errors.
pub async fn upload(
	runtime: &SigningHostRuntime,
	network: &Stack,
	entropy: &[u8],
	username: &str,
	path: &Path,
) -> Result<String> {
	let path = path.to_owned();
	let image = tokio::task::spawn_blocking(move || read_image(&path))
		.await
		.context("join profile image reader")??;
	let mut profile = chain::ChainProfile::connect(runtime, network, entropy, username).await?;
	let record = publish_avatar(&mut profile.backend(), image).await?;
	let published = PublicProfile { avatar: record.avatar() };
	Ok(format!("Profile published\n{}", format_profile(username, &published)))
}

fn format_profile(username: &str, profile: &PublicProfile) -> String {
	match &profile.avatar {
		Some(avatar) => format!("{username}\nAvatar: {} ({:?})", avatar.cid, avatar.format),
		None => format!("{username}\nNo avatar"),
	}
}

fn read_image(path: &Path) -> Result<(ImageFormat, Vec<u8>)> {
	let mut bytes = Vec::new();
	File::open(path)
		.with_context(|| format!("open avatar {}", path.display()))?
		.take(u64::try_from(MAX_IMAGE_BYTES)? + 1)
		.read_to_end(&mut bytes)
		.with_context(|| format!("read avatar {}", path.display()))?;
	let format =
		check_image(&bytes, None).map_err(|rejection| anyhow!("image rejected: {rejection}"))?;
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
	async fn prepare(&mut self, text: &str) -> Result<Self::Prepared>;
	async fn upload(&mut self, bytes: Vec<u8>) -> Result<[u8; 32]>;
	async fn publish(&mut self, prepared: Self::Prepared) -> Result<()>;
}

async fn publish_avatar(
	backend: &mut impl ProfileBackend,
	(format, bytes): (ImageFormat, Vec<u8>),
) -> Result<ProfileRecord> {
	let original = backend.read().await?;
	let mut record = match original.as_deref() {
		Some(text) => ProfileRecord::from_text(text)
			.context("existing profile record is malformed; refusing to overwrite it")?,
		None => ProfileRecord::default(),
	};
	let key = preimage_key(&bytes);
	record.set_avatar(ProfileAvatar { cid: key.into(), format });
	let text = record.to_text().context("profile record is too large for readers to decode")?;
	let prepared = backend.prepare(&text).await?;
	ensure!(backend.upload(bytes).await? == key, "uploaded avatar key does not match its bytes");
	ensure!(
		backend.read().await? == original,
		"profile changed during the upload; retry to preserve the new record"
	);
	backend.publish(prepared).await?;
	ensure!(
		backend.read().await?.as_deref() == Some(text.as_str()),
		"published profile does not match the submitted record"
	);
	Ok(record)
}

#[cfg(test)]
mod tests;
