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
#[derive(Debug, derive_more::Display)]
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
mod tests {
	use trinity_stack::DEFAULT_LINK_BASE;

	use super::*;

	#[test]
	fn a_key_that_contradicts_the_link_is_refused() {
		let key = format!("0x{}", hex::encode(trinity_stack::public_key(&[7; 32])));
		let other = format!("0x{}", hex::encode(trinity_stack::public_key(&[9; 32])));
		let link = Pointer::new("https://a.example/s.json", Some(&key))
			.unwrap()
			.to_link(DEFAULT_LINK_BASE);
		assert!(pointer_of(&link, Some(&other)).is_err());
		assert_eq!(
			pointer_of(&link, Some(&key)).unwrap().unwrap().key(),
			Pointer::from_link(&link).unwrap().key()
		);
		let unkeyed = Pointer::new("https://a.example/s.json", None)
			.unwrap()
			.to_link(DEFAULT_LINK_BASE);
		let filled = pointer_of(&unkeyed, Some(&key)).unwrap().unwrap();
		assert_eq!(filled.key().map(hex::encode), Some(key[2..].to_owned()));
		assert!(pointer_of("./stack.json", None).unwrap().is_none());
	}

	#[test]
	fn a_link_or_url_breaking_a_rule_says_which_instead_of_reading_a_file() {
		let code = |stack: &str| pointer_of(stack, None).unwrap_err().to_string();
		let plain_http = "polkadotapp://stack?url=http%3A%2F%2Fstack.example%2Fs.json";
		assert!(code(plain_http).contains("invalid-url"), "{}", code(plain_http));
		let short_key = "polkadotapp://stack?key=0x12&url=https%3A%2F%2Fstack.example%2Fs.json";
		assert!(code(short_key).contains("invalid-hex"), "{}", code(short_key));
		assert!(code("http://stack.example/s.json").contains("invalid-url"));
		let app_link = "https://app.example/open?url=https%3A%2F%2Fstack.example%2Fs.json";
		assert_eq!(
			pointer_of(app_link, None).unwrap().unwrap().url().as_str(),
			"https://stack.example/s.json"
		);
	}

	/// A server that answers with an error status is told apart from one that
	/// does not answer, so a moved descriptor can be reported louder.
	#[tokio::test]
	async fn test_fetch_http_status_error() {
		use tokio::io::{AsyncReadExt, AsyncWriteExt};

		let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
		let url = format!("http://{}/stack.json", listener.local_addr().unwrap());
		tokio::spawn(async move {
			let (mut socket, _) = listener.accept().await.unwrap();
			let mut request = [0; 1024];
			let _ = socket.read(&mut request).await;
			let response =
				"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
			socket.write_all(response.as_bytes()).await.unwrap();
		});
		let error = fetch(&url, FETCH_TIMEOUT).await.expect_err("404 accepted");
		assert!(matches!(&error, FetchError::Http(status) if status.contains("404")));

		let closed_port_error = fetch("http://127.0.0.1:9/stack.json", FETCH_TIMEOUT)
			.await
			.expect_err("closed port accepted");
		assert!(matches!(closed_port_error, FetchError::Transport(_)));
	}
}
