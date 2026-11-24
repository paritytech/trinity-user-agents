//! Reading Trinity stack descriptors: where a stack value points, fetching and
//! loading its descriptor, and the roles a stack-derived network serves.

use std::{
	fs,
	path::PathBuf,
	time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow, bail};
use reqwest::Url;
use trinity_stack::{Chain, Descriptor, Fetched, Pointer, accept};

/// A local file has no URL of its own; the pointer only carries the key to check. Every local
/// file is read under it, so its `stack_id` does not tell one file's stack from another's.
const LOCAL_FILE_URL: &str = "https://local-file.invalid/";
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
/// The largest body `fetch` reads. A descriptor is a few kilobytes, and the
/// largest light spec, a relay's with its checkpoint, is under 300 KiB.
const MAX_FETCH_BYTES: usize = 1024 * 1024;

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
/// key a link carries or `key`, or a local descriptor file, signed or not. Fails when it is
/// unreachable, breaks a rule or is past `expiresAt`; warns when it is past `validUntil`.
pub async fn load_descriptor(stack: &str, key: Option<&str>) -> Result<Descriptor> {
	let (pointer, bytes) = read_stack(stack, key).await?;
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

/// The id of the stack a `--stack` value names, read with `key`: the id of the pointer a link or
/// a URL carries. A local file has no pointer of its own, so its id is `file:` followed by its
/// canonical path: two files are two stacks, and a file that moves becomes another stack. A file
/// whose canonical path is not UTF-8 is refused.
pub fn stack_id(stack: &str, key: Option<&str>) -> Result<String> {
	match pointer_of(stack, key)? {
		Some(pointer) => Ok(pointer.stack_id()),
		None => local_file_id(stack),
	}
}

fn local_file_id(path: &str) -> Result<String> {
	let canonical = fs::canonicalize(path).with_context(|| format!("cannot read {path}"))?;
	canonical_file_id(canonical)
}

/// `file:` followed by `canonical`. Fails when `canonical` is not UTF-8, since
/// a lossy conversion would give two such paths one id.
fn canonical_file_id(canonical: PathBuf) -> Result<String> {
	let canonical = canonical.into_os_string().into_string().map_err(|canonical| {
		anyhow!("{} is not a UTF-8 path, so it cannot name a stack", canonical.display())
	})?;
	Ok(format!("file:{canonical}"))
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
	/// Each role's value through a fallible `transform`; fails with its first
	/// error.
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

	/// Each role's value, People first, then Asset Hub, then Bulletin.
	pub fn iter(&self) -> impl Iterator<Item = &T> {
		[&self.people, &self.asset_hub, &self.bulletin].into_iter()
	}
}

/// The chains of `descriptor` the light client runs: the relay, then People,
/// Asset Hub and Bulletin. One the descriptor does not have is left out.
pub fn light_client_chains(descriptor: &Descriptor) -> impl Iterator<Item = &Chain> {
	let roles = ROLE_NAMES.iter().filter_map(|role| descriptor.chain(role));
	descriptor.relay().into_iter().chain(roles)
}

/// Whether `stack` and `other` agree in every field the CLI or the core reads
/// apart from the light specs: `id`, which is the product namespace, the TLD,
/// `generation`, the `dub` service, `dotns.popController`, `contracts`, and the
/// genesis hash and full `rpc` list of People, Asset Hub and Bulletin. The full
/// lists are compared because the Polkadot fork check contacts every node the
/// reference lists for a chain. A light spec is left out because a relay's
/// rotates with every fresh checkpoint and the light client fetches the one
/// the descriptor names itself.
pub fn reads_same(stack: &Descriptor, other: &Descriptor) -> bool {
	fn role_chain<'a>(descriptor: &'a Descriptor, role: &str) -> Option<([u8; 32], &'a [String])> {
		descriptor.chain(role).map(|chain| (chain.genesis(), chain.rpc.as_slice()))
	}
	stack.id == other.id &&
		stack.tld == other.tld &&
		stack.generation == other.generation &&
		stack.service("dub") == other.service("dub") &&
		stack.dotns_address("popController") == other.dotns_address("popController") &&
		stack.contracts == other.contracts &&
		ROLE_NAMES.iter().all(|role| role_chain(stack, role) == role_chain(other, role))
}

/// Whether a bundled copy of `stack` can stay in place of `other`: they
/// [read the same](reads_same) and name the same `lightSpec.sha256` for the
/// relay, People, Asset Hub and Bulletin, whose specs the binary ships.
pub fn bundles_same(stack: &Descriptor, other: &Descriptor) -> bool {
	fn light_specs(descriptor: &Descriptor) -> impl Iterator<Item = Option<&str>> {
		light_client_chains(descriptor)
			.map(|chain| chain.light_spec.as_ref().map(|spec| spec.sha256.as_str()))
	}
	reads_same(stack, other) && light_specs(stack).eq(light_specs(other))
}

/// Why `fetched` cannot replace `stored`, the copy a stack already runs on:
/// a change to the TLD, which every account derives under, or to the `id`,
/// which the CLI names the network by. `None` when both stay.
pub fn describe_pin_change(stored: &Descriptor, fetched: &Descriptor) -> Option<String> {
	if fetched.tld != stored.tld {
		Some(format!(
			"changes the tld from {} to {}, which changes every account derived under it",
			stored.tld, fetched.tld
		))
	} else if fetched.id != stored.id {
		Some(format!(
			"changes the id from {} to {}, which the CLI names the network by",
			stored.id, fetched.id
		))
	} else {
		None
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
	/// The body is larger than `fetch` reads.
	#[display("larger than {MAX_FETCH_BYTES} bytes")]
	TooLarge,
}

/// The bytes a descriptor or light spec URL serves, or why it is unreachable. Redirects are
/// refused: the URL rules hold for the URL the pointer names, and a redirect could lead to plain
/// http.
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
	if response.content_length().is_some_and(|length| length > MAX_FETCH_BYTES as u64) {
		return Err(FetchError::TooLarge);
	}
	let mut response = response;
	let mut body = Vec::new();
	while let Some(chunk) = response.chunk().await.map_err(transport)? {
		body.extend_from_slice(&chunk);
		if body.len() > MAX_FETCH_BYTES {
			return Err(FetchError::TooLarge);
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
