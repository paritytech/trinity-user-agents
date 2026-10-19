//! `truapi-host stack`: publish, check and bundle Trinity stack descriptors with the
//! `trinity-stack` crate, the same rules the host core runs.

use std::{
	fs,
	io::Write,
	path::{Path, PathBuf},
	time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow, bail};
use clap::Subcommand;
use qrcode::{QrCode, render::unicode::Dense1x2};
use serde::{Deserialize, Serialize};
use trinity_stack::{DEFAULT_LINK_BASE, Document, Fetched, Pointer, Status, accept};
use zeroize::Zeroizing;

/// Where `publish` writes the descriptor under its output directory.
const WELL_KNOWN: &str = ".well-known/trinity-stack.json";
/// A local file has no URL of its own; the pointer only carries the key to check.
const LOCAL_FILE_URL: &str = "https://local-file.invalid/";
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);
/// The largest descriptor `fetch` reads; a descriptor is a few kilobytes.
const MAX_DESCRIPTOR_BYTES: usize = 1024 * 1024;

#[derive(Subcommand)]
pub enum StackCommand {
	/// Write a new signing key to a file and print its public key.
	Keygen {
		/// File to create; refused if it exists. Readable by its owner only.
		#[arg(long)]
		out: PathBuf,
	},
	/// Render a model, check it, sign it when given a key, and write
	/// `<out>/.well-known/trinity-stack.json` for a static host.
	Publish {
		/// A descriptor without `$v`, `sequence` and `generation`.
		model: PathBuf,
		/// The stack's current generation, changed whenever its chains restart.
		#[arg(long)]
		generation: String,
		/// Directory to write under.
		#[arg(long)]
		out: PathBuf,
		/// File holding the 0x-prefixed secret key, as `keygen` writes it.
		#[arg(long)]
		signing_key_file: Option<PathBuf>,
		/// File that keeps `sequence` between publishes. Keep it outside anything
		/// a wipe deletes.
		#[arg(long)]
		state: Option<PathBuf>,
		/// Where the descriptor will be served, to print its link.
		#[arg(long)]
		url: Option<String>,
	},
	/// Check a descriptor and print it with whether it is signed, stale or expired.
	///
	/// Exits non-zero when the descriptor breaks a rule or the stack is past `expiresAt`, so a
	/// monitoring job can rely on the exit code. A stale copy is reported, not failed.
	Verify {
		/// A link, a URL or a local file.
		stack: String,
		/// The public key the descriptor must be signed with.
		#[arg(long)]
		key: Option<String>,
	},
	/// Fetch and verify a signed stack, and write the copy a host build ships.
	///
	/// The previous copy at `--out` counts as stored, so a rebundle never lowers
	/// `sequence`. Refuses an expired copy.
	Bundle {
		/// A link or a URL.
		stack: String,
		/// The public key the descriptor must be signed with.
		#[arg(long)]
		key: Option<String>,
		/// File to write the bundled copy to.
		#[arg(long)]
		out: PathBuf,
	},
	/// Print the link that carries a pointer, and its QR code.
	Link {
		/// The descriptor's URL.
		url: String,
		/// The stack's public key, when it is signed.
		#[arg(long)]
		key: Option<String>,
	},
}

/// Run one `stack` subcommand.
pub async fn run(command: StackCommand) -> Result<()> {
	match command {
		StackCommand::Keygen { out } => keygen(&out),
		StackCommand::Publish { model, generation, out, signing_key_file, state, url } => {
			let secret = signing_key_file.as_deref().map(read_secret).transpose()?;
			let secret = secret.as_deref();
			publish(&model, &generation, &out, secret, state.as_deref(), url.as_deref())
		},
		StackCommand::Verify { stack, key } => verify(&stack, key.as_deref()).await,
		StackCommand::Bundle { stack, key, out } => bundle(&stack, key.as_deref(), &out).await,
		StackCommand::Link { url, key } => {
			let link =
				Pointer::new(&url, key.as_deref()).map_err(refused)?.to_link(DEFAULT_LINK_BASE);
			let qr = QrCode::new(link.as_bytes()).context("link does not fit in a QR code")?;
			println!("{link}\n{}", qr.render::<Dense1x2>().quiet_zone(true).build());
			Ok(())
		},
	}
}

fn refused(error: trinity_stack::Error) -> anyhow::Error {
	anyhow!("refused ({}): {error}", error.code())
}

fn keygen(out: &Path) -> Result<()> {
	let mut secret = Zeroizing::new([0u8; 32]);
	getrandom::getrandom(secret.as_mut()).map_err(|error| anyhow!("no randomness: {error}"))?;
	let mut options = fs::OpenOptions::new();
	options.write(true).create_new(true);
	#[cfg(unix)]
	std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
	let mut file = options.open(out).with_context(|| format!("create {}", out.display()))?;
	let encoded = Zeroizing::new(format!("0x{}\n", hex::encode(secret.as_ref())));
	file.write_all(encoded.as_bytes())?;
	println!("0x{}", hex::encode(trinity_stack::public_key(&secret)));
	Ok(())
}

fn read_secret(path: &Path) -> Result<Zeroizing<[u8; 32]>> {
	let text = Zeroizing::new(
		fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?,
	);
	let digits = text.trim().strip_prefix("0x").unwrap_or(text.trim());
	let mut secret = Zeroizing::new([0u8; 32]);
	hex::decode_to_slice(digits, secret.as_mut())
		.with_context(|| format!("{} is not a 32-byte hex secret key", path.display()))?;
	Ok(secret)
}

/// What `publish` remembers between runs.
#[derive(Serialize, Deserialize, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
struct PublishState {
	sequence: u64,
	content_hash: String,
}

fn publish(
	model: &Path,
	generation: &str,
	out: &Path,
	secret: Option<&[u8; 32]>,
	state_file: Option<&Path>,
	url: Option<&str>,
) -> Result<()> {
	let link = url
		.map(|url| {
			let key = secret
				.map(|secret| format!("0x{}", hex::encode(trinity_stack::public_key(secret))));
			Pointer::new(url, key.as_deref()).map(|pointer| pointer.to_link(DEFAULT_LINK_BASE))
		})
		.transpose()
		.map_err(refused)?;
	let model = fs::read(model).with_context(|| format!("read {}", model.display()))?;
	let model: serde_json::Map<String, serde_json::Value> =
		serde_json::from_slice(&model).context("the model is not a JSON object")?;
	let state = match state_file {
		Some(path) if path.exists() => {
			let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
			Some(
				serde_json::from_slice::<PublishState>(&bytes)
					.with_context(|| format!("{} is not a publish state", path.display()))?,
			)
		},
		_ => None,
	};
	let (descriptor, state) = render(model, generation, state, unix_now())?;
	let document = match secret {
		Some(secret) => trinity_stack::sign(&descriptor, secret).map_err(refused)?,
		None => descriptor,
	};
	let path = out.join(WELL_KNOWN);
	fs::create_dir_all(path.parent().expect("WELL_KNOWN has a parent"))?;
	// The state goes first: if the descriptor write then fails, a retry repeats the same
	// `sequence` rather than skipping one the readers never saw.
	if let Some(state_file) = state_file {
		write_atomically(state_file, &serde_json::to_vec_pretty(&state)?)?;
	}
	write_atomically(&path, &document)?;
	println!("wrote {}, sequence {}", path.display(), state.sequence);
	if let Some(link) = link {
		println!("{link}");
	}
	Ok(())
}

/// Replaces `path` in one step, so a web root or a build never sees half a file.
fn write_atomically(path: &Path, contents: &[u8]) -> Result<()> {
	let directory = path.parent().filter(|parent| !parent.as_os_str().is_empty());
	let mut file = tempfile::NamedTempFile::new_in(directory.unwrap_or(Path::new(".")))
		.with_context(|| format!("write {}", path.display()))?;
	file.write_all(contents)?;
	file.persist(path).with_context(|| format!("write {}", path.display()))?;
	Ok(())
}

/// Renders a model as a `$v: 1` descriptor and checks it. `sequence` stays the same while nothing
/// changes and otherwise rises to the larger of the last one plus one and `now`, so a publisher
/// with no saved state still never goes backwards.
fn render(
	mut model: serde_json::Map<String, serde_json::Value>,
	generation: &str,
	state: Option<PublishState>,
	now: u64,
) -> Result<(Vec<u8>, PublishState)> {
	for owned in ["$v", "sequence", "generation"] {
		model.remove(owned);
	}
	model.insert("$v".into(), trinity_stack::VERSION.into());
	model.insert("generation".into(), generation.into());
	model.insert("sequence".into(), 0.into());
	// The crate hashes and renders the canonical form, as `@parity/trinity-stack`'s `publish` does,
	// so equal hashes mean equal published bytes and a state file means the same thing to both.
	let hash = trinity_stack::content_hash(&serde_json::to_vec(&model)?).map_err(refused)?;
	let content_hash = format!("0x{}", hex::encode(hash));
	let sequence = match state {
		Some(state) if state.content_hash == content_hash => state.sequence,
		Some(state) => (state.sequence + 1).max(now),
		None => now,
	};
	model.insert("sequence".into(), sequence.into());
	let descriptor = trinity_stack::canonical(&serde_json::to_vec(&model)?).map_err(refused)?;
	Ok((descriptor, PublishState { sequence, content_hash }))
}

/// A link or URL as a pointer, with `key` filling in a link that has none. `None` for a path.
fn pointer_of(stack: &str, key: Option<&str>) -> Result<Option<Pointer>> {
	if let Ok(linked) = Pointer::from_link(stack) {
		let Some(given) = key else { return Ok(Some(linked)) };
		let keyed = Pointer::new(linked.url().as_str(), Some(given)).map_err(refused)?;
		if linked.key().is_some_and(|own| Some(own) != keyed.key()) {
			bail!("--key contradicts the key the link carries");
		}
		return Ok(Some(keyed));
	}
	if stack.starts_with("https://") || stack.starts_with("http://") {
		return Ok(Some(Pointer::new(stack, key).map_err(refused)?));
	}
	Ok(None)
}

async fn verify(stack: &str, key: Option<&str>) -> Result<()> {
	let (pointer, fetched) = match pointer_of(stack, key)? {
		Some(pointer) => {
			let fetched = fetch(pointer.url().as_str()).await;
			(pointer, fetched)
		},
		None => (
			Pointer::new(LOCAL_FILE_URL, key).map_err(refused)?,
			Ok(fs::read(stack).with_context(|| format!("read {stack}"))?),
		),
	};
	let bytes = fetched
		.as_deref()
		.map_err(|cause| anyhow!("{} is unreachable: {cause}", pointer.url()))?;
	let accepted =
		accept(&pointer, None, Fetched::Document(bytes), unix_now() as i64).map_err(refused)?;
	// `signed` means a signature was checked against a key; `envelope` says the file carries one,
	// so a signed stack read without `--key` is not mistaken for an unsigned one.
	let envelope = matches!(Document::parse(bytes), Ok(Document::Envelope { .. }));
	let report = serde_json::json!({
		"signed": accepted.stack.signed,
		"envelope": envelope,
		"stale": accepted.stale,
		"expired": accepted.expired,
		"descriptor": accepted.stack.descriptor,
	});
	println!("{}", serde_json::to_string_pretty(&report)?);
	if accepted.expired {
		bail!("{stack} is past expiresAt");
	}
	Ok(())
}

async fn bundle(stack: &str, key: Option<&str>, out: &Path) -> Result<()> {
	let Some(pointer) = pointer_of(stack, key)?.filter(|pointer| pointer.key().is_some()) else {
		bail!("bundle needs a signed stack: a link or URL with a key");
	};
	let previous = match fs::read(out) {
		Ok(bytes) => Some(bytes),
		Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
		Err(error) => return Err(error).with_context(|| format!("read {}", out.display())),
	};
	let fetched = fetch(pointer.url().as_str()).await;
	if let Err(cause) = &fetched {
		bail!("not bundled: {} is unreachable: {cause}", pointer.url());
	}
	let fetched = fetched.as_deref().map_or(Fetched::Unreachable, Fetched::Document);
	let accepted =
		accept(&pointer, previous.as_deref(), fetched, unix_now() as i64).map_err(refused)?;
	match &accepted.status {
		Status::Updated | Status::Unchanged => {},
		Status::Refused(error) => {
			bail!("not bundled: {}, {} is unchanged", refused(error.clone()), out.display())
		},
		status => bail!("not bundled: {}, {} is unchanged", status.code(), out.display()),
	}
	if accepted.expired {
		bail!("not bundled: {} is past expiresAt", pointer.url());
	}
	if accepted.stale {
		eprintln!("warning: {} is past validUntil; its operator should republish", pointer.url());
	}
	write_atomically(out, &accepted.document)?;
	let descriptor = &accepted.stack.descriptor;
	println!("wrote {}, {} sequence {}", out.display(), descriptor.id, descriptor.sequence);
	Ok(())
}

/// The bytes a descriptor URL serves, or why it is unreachable. Redirects are refused: the URL
/// rules hold for the URL the pointer names, and a redirect could lead to plain http.
async fn fetch(url: &str) -> Result<Vec<u8>, String> {
	let client = reqwest::Client::builder()
		.timeout(FETCH_TIMEOUT)
		.redirect(reqwest::redirect::Policy::none())
		.build()
		.map_err(|error| error.to_string())?;
	let response = client.get(url).send().await.map_err(|error| match error.is_timeout() {
		true => format!("no answer within {}s", FETCH_TIMEOUT.as_secs()),
		false => error.to_string(),
	})?;
	let status = response.status();
	if status.is_redirection() {
		let location = response.headers().get(reqwest::header::LOCATION);
		let location = location.and_then(|value| value.to_str().ok()).unwrap_or("nowhere");
		return Err(format!("HTTP {status} to {location}; redirects are not followed"));
	}
	if !status.is_success() {
		return Err(format!("HTTP {status}"));
	}
	let too_large = || format!("larger than {MAX_DESCRIPTOR_BYTES} bytes");
	if response
		.content_length()
		.is_some_and(|length| length > MAX_DESCRIPTOR_BYTES as u64)
	{
		return Err(too_large());
	}
	let mut response = response;
	let mut body = Vec::new();
	while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
		body.extend_from_slice(&chunk);
		if body.len() > MAX_DESCRIPTOR_BYTES {
			return Err(too_large());
		}
	}
	Ok(body)
}

fn unix_now() -> u64 {
	SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs())
}

#[cfg(test)]
mod tests {
	use super::*;

	fn model() -> serde_json::Map<String, serde_json::Value> {
		serde_json::from_value(serde_json::json!({
			"id": "local", "name": "Local", "network": "local", "tld": "local", "services": [],
			"chains": [{
				"role": "Relay",
				"genesisHash": "0x860145753657e73c29b9388ffa0a8aebc643ea87434b4b271b6c3c3cc9e6bf92",
				"rpc": ["ws://127.0.0.1:9944"],
			}],
		}))
		.unwrap()
	}

	#[test]
	fn sequence_holds_while_nothing_changes_and_rises_when_something_does() {
		let (_, first) = render(model(), "g1", None, 1_000).unwrap();
		assert_eq!(first.sequence, 1_000);
		let (_, again) = render(model(), "g1", Some(first), 2_000).unwrap();
		assert_eq!(again.sequence, 1_000, "republishing the same content keeps sequence");
		let (_, new_generation) = render(model(), "g2", Some(again), 900).unwrap();
		assert_eq!(new_generation.sequence, 1_001, "a clock behind the state still moves forward");
	}

	#[test]
	fn the_content_hash_is_the_crates() {
		let (descriptor, state) = render(model(), "g1", None, 1_000).unwrap();
		let expected =
			format!("0x{}", hex::encode(trinity_stack::content_hash(&descriptor).unwrap()));
		assert_eq!(state.content_hash, expected, "the npm package's publish keeps the same hash");
	}

	#[test]
	fn a_service_kind_only_newer_readers_know_raises_sequence() {
		let (_, first) = render(model(), "g1", None, 1_000).unwrap();
		let mut future = model();
		future["services"] =
			serde_json::json!([{ "kind": "future-kind", "urls": ["https://a.example/"] }]);
		let (_, again) = render(future, "g1", Some(first), 900).unwrap();
		assert_eq!(again.sequence, 1_001);
	}

	#[test]
	fn a_model_cannot_set_the_fields_publish_owns() {
		let mut stale = model();
		stale.insert("sequence".into(), 7.into());
		stale.insert("generation".into(), "old".into());
		let (descriptor, state) = render(stale, "g1", None, 1_000).unwrap();
		let parsed = Document::parse(&descriptor).unwrap();
		assert_eq!(
			(parsed.descriptor().sequence, parsed.descriptor().generation.as_str()),
			(1_000, "g1")
		);
		assert_eq!(state.sequence, 1_000);
	}

	#[test]
	fn a_model_that_breaks_a_rule_is_refused_before_anything_is_written() {
		let mut broken = model();
		broken.insert("tld".into(), ".dot".into());
		let error = render(broken, "g1", None, 1_000).unwrap_err();
		assert!(error.to_string().contains("invalid-tld"), "{error}");
	}

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
}
