//! Reading Trinity stack descriptors: where a stack value points, fetching and
//! loading its descriptor, and the fields of it a stack-derived network reads.

use std::{
	fs,
	time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow, bail};
use reqwest::Url;
use trinity_stack::{Descriptor, Fetched, Pointer, accept};

/// A local file has no URL of its own; the pointer only carries the key to check.
const LOCAL_FILE_URL: &str = "https://local-file.invalid/";
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
/// The largest descriptor `fetch` reads; a descriptor is a few kilobytes.
const MAX_DESCRIPTOR_BYTES: usize = 1024 * 1024;

pub fn refused(error: trinity_stack::Error) -> anyhow::Error {
	anyhow!("refused ({}): {error}", error.code())
}

/// A link (any URL with a `url` parameter) or a descriptor URL as a pointer, with `key` filling in
/// a link that has none. `None` for a path, which never parses as an absolute URL.
pub fn pointer_of(stack: &str, key: Option<&str>) -> Result<Option<Pointer>> {
	let Ok(parsed) = Url::parse(stack) else { return Ok(None) };
	if !parsed.query_pairs().any(|(name, _)| name == "url") {
		return Ok(Some(Pointer::new(stack, key).map_err(refused)?));
	}
	let linked = Pointer::from_link(stack).map_err(refused)?;
	let Some(given) = key else { return Ok(Some(linked)) };
	let keyed = Pointer::new(linked.url().as_str(), Some(given)).map_err(refused)?;
	if linked.key().is_some_and(|own| Some(own) != keyed.key()) {
		bail!("--key contradicts the key the link carries");
	}
	Ok(Some(keyed))
}

/// The pointer a stack value carries and the bytes it serves: a link or a URL, fetched, or a local
/// file, read under a pointer with no URL of its own and `key`. Fails when it is unreachable.
pub async fn read_stack(stack: &str, key: Option<&str>) -> Result<(Pointer, Vec<u8>)> {
	match pointer_of(stack, key)? {
		Some(pointer) => {
			let bytes = fetch(pointer.url().as_str(), FETCH_TIMEOUT)
				.await
				.map_err(|cause| anyhow!("{stack} is unreachable: {cause}"))?;
			Ok((pointer, bytes))
		},
		None => Ok((
			Pointer::new(LOCAL_FILE_URL, key).map_err(refused)?,
			fs::read(stack).with_context(|| format!("cannot read {stack}"))?,
		)),
	}
}

/// The descriptor a `--stack` value points to: a link or a URL, fetched and checked against the
/// key a link carries, or a local descriptor file, signed or not. Fails when it is unreachable,
/// breaks a rule or is past `expiresAt`; warns when it is past `validUntil`.
pub async fn load_descriptor(stack: &str) -> Result<Descriptor> {
	let (pointer, bytes) = read_stack(stack, None).await?;
	let accepted = accept(&pointer, None, Fetched::Document(&bytes), unix_now() as i64)
		.map_err(|error| anyhow!("stack {stack} is {}", refused(error)))?;
	if accepted.expired {
		bail!("stack {stack} is past expiresAt");
	}
	if accepted.stale {
		tracing::warn!("stack {stack} is past validUntil; its operator should republish");
	}
	Ok(accepted.stack.descriptor)
}

/// One value for each chain role a host routes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Roles<T> {
	pub people: T,
	pub asset_hub: T,
	pub bulletin: T,
}

/// Each role's `role` in a descriptor's `chains`.
pub const ROLE_NAMES: Roles<&str> =
	Roles { people: "People", asset_hub: "AssetHub", bulletin: "Bulletin" };

impl<T> Roles<T> {
	pub fn map<U>(self, mut transform: impl FnMut(T) -> U) -> Roles<U> {
		Roles {
			people: transform(self.people),
			asset_hub: transform(self.asset_hub),
			bulletin: transform(self.bulletin),
		}
	}

	/// [`Self::map`] with a fallible `transform`; fails with its first error.
	pub fn try_map<U, E>(
		self,
		mut transform: impl FnMut(T) -> Result<U, E>,
	) -> Result<Roles<U>, E> {
		Ok(Roles {
			people: transform(self.people)?,
			asset_hub: transform(self.asset_hub)?,
			bulletin: transform(self.bulletin)?,
		})
	}

	pub fn zip<U>(self, other: Roles<U>) -> Roles<(T, U)> {
		Roles {
			people: (self.people, other.people),
			asset_hub: (self.asset_hub, other.asset_hub),
			bulletin: (self.bulletin, other.bulletin),
		}
	}

	/// Each role's value, People first, then Asset Hub, then Bulletin.
	pub fn iter(&self) -> impl Iterator<Item = &T> {
		[&self.people, &self.asset_hub, &self.bulletin].into_iter()
	}

	/// Each role's value, borrowed.
	pub fn as_ref(&self) -> Roles<&T> {
		Roles { people: &self.people, asset_hub: &self.asset_hub, bulletin: &self.bulletin }
	}
}

/// The descriptor fields a stack-derived network is built from, and
/// `generation`. Only a change to these makes the runtime notice print.
#[derive(Debug, PartialEq, Eq)]
pub struct StackProjection<'a> {
	pub generation: &'a str,
	pub tld: &'a str,
	/// `rpc[0]` and `genesisHash` of each role's chain, when present.
	pub roles: Roles<Option<(&'a str, &'a str)>>,
	pub dub: Option<&'a str>,
	pub pop_controller: Option<&'a str>,
}

impl<'a> StackProjection<'a> {
	pub fn of(descriptor: &'a Descriptor) -> Self {
		Self {
			generation: &descriptor.generation,
			tld: &descriptor.tld,
			roles: ROLE_NAMES.map(|role| {
				let chain = descriptor.chain(role)?;
				Some((chain.rpc[0].as_str(), chain.genesis_hash.as_str()))
			}),
			dub: descriptor
				.services
				.iter()
				.find(|service| service.kind == "dub")
				.and_then(|service| service.urls.first())
				.map(String::as_str),
			pop_controller: descriptor
				.dotns
				.as_ref()
				.and_then(|dotns| dotns.get("popController"))
				.map(String::as_str),
		}
	}
}

/// Why a descriptor URL served no descriptor.
#[derive(derive_more::Display)]
pub enum FetchError {
	/// The server returned a status other than 2xx.
	#[display("{_0}")]
	Http(String),
	/// No answer, or a body that cannot be a descriptor.
	#[display("{_0}")]
	Transport(String),
}

/// The bytes a descriptor URL serves, or why it is unreachable. Redirects are refused: the URL
/// rules hold for the URL the pointer names, and a redirect could lead to plain http.
pub async fn fetch(url: &str, timeout: Duration) -> Result<Vec<u8>, FetchError> {
	let transport = |error: reqwest::Error| FetchError::Transport(error.to_string());
	let client = reqwest::Client::builder()
		.timeout(timeout)
		.redirect(reqwest::redirect::Policy::none())
		.build()
		.map_err(transport)?;
	let response = client.get(url).send().await.map_err(|error| match error.is_timeout() {
		true => FetchError::Transport(format!("no answer within {}s", timeout.as_secs())),
		false => transport(error),
	})?;
	let status = response.status();
	if status.is_redirection() {
		let location = response.headers().get(reqwest::header::LOCATION);
		let location = location.and_then(|value| value.to_str().ok()).unwrap_or("nowhere");
		return Err(FetchError::Http(format!(
			"HTTP {status} to {location}; redirects are not followed"
		)));
	}
	if !status.is_success() {
		return Err(FetchError::Http(format!("HTTP {status}")));
	}
	let too_large = || FetchError::Transport(format!("larger than {MAX_DESCRIPTOR_BYTES} bytes"));
	if response
		.content_length()
		.is_some_and(|length| length > MAX_DESCRIPTOR_BYTES as u64)
	{
		return Err(too_large());
	}
	let mut response = response;
	let mut body = Vec::new();
	while let Some(chunk) = response.chunk().await.map_err(transport)? {
		body.extend_from_slice(&chunk);
		if body.len() > MAX_DESCRIPTOR_BYTES {
			return Err(too_large());
		}
	}
	Ok(body)
}

/// Seconds since the Unix epoch, or 0 when the clock is before it.
pub fn unix_now() -> u64 {
	SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs())
}

#[cfg(test)]
mod tests;
