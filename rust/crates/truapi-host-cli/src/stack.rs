//! `truapi-host stack`: publish, check and bundle Trinity stack descriptors with the
//! `trinity-stack` crate, the same rules the host core runs.

use std::{
	fs,
	io::{IsTerminal, Write},
	path::{Path, PathBuf},
	time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, anyhow, bail};
use clap::Subcommand;
use crossterm::style::Stylize;
use reqwest::Url;
use trinity_stack::{
	DEFAULT_LINK_BASE, Document, Fetched, Pointer, PublishState, Rendered, Status, accept, render,
};
use zeroize::Zeroizing;

use crate::{platform, terminal_ui::qr_rows};

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
			println!("{link}");
			let terminal = std::io::stdout().is_terminal();
			for row in qr_rows(&link).context("link does not fit in a QR code")? {
				match terminal {
					true => println!("{}", row.black().on_white()),
					false => println!("{row}"),
				}
			}
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
	if let Some(parent) = out.parent().filter(|parent| !parent.as_os_str().is_empty()) {
		fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
	}
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
	let previous = match state_file {
		Some(path) if path.exists() => {
			let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
			Some(
				serde_json::from_slice::<PublishState>(&bytes)
					.with_context(|| format!("{} is not a publish state", path.display()))?,
			)
		},
		_ => None,
	};
	let Rendered { plain, state } =
		render(&model, generation, previous.as_ref(), unix_now()).map_err(refused)?;
	let document = match secret {
		Some(secret) => trinity_stack::sign(&plain, secret).map_err(refused)?,
		None => plain,
	};
	let path = out.join(WELL_KNOWN);
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

/// Replaces `path` in one step, so a web root or a build never sees half a file. The file gets the
/// umask's permissions, so a web server running as another user can serve it.
fn write_atomically(path: &Path, contents: &[u8]) -> Result<()> {
	platform::atomic_write(path, contents).map_err(anyhow::Error::msg)
}

/// A link (any URL with a `url` parameter) or a descriptor URL as a pointer, with `key` filling in
/// a link that has none. `None` for a path, which never parses as an absolute URL.
fn pointer_of(stack: &str, key: Option<&str>) -> Result<Option<Pointer>> {
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
	let fetched = fetch(pointer.url().as_str())
		.await
		.map_err(|cause| anyhow!("not bundled: {} is unreachable: {cause}", pointer.url()))?;
	let accepted =
		accept(&pointer, previous.as_deref(), Fetched::Document(&fetched), unix_now() as i64)
			.map_err(refused)?;
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
}
