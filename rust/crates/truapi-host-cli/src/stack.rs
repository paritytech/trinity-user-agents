//! `truapi-host stack`: publish, check and bundle Trinity stack descriptors with the
//! `trinity-stack` crate, the same rules the host core runs.

use std::{
	fs,
	io::{IsTerminal, Write},
	path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, bail};
use clap::Subcommand;
use crossterm::style::Stylize;
use trinity_stack::{
	DEFAULT_LINK_BASE, Document, Fetched, Pointer, PublishState, Rendered, Status, accept, open,
	render,
};
use zeroize::Zeroizing;

use crate::{
	network::Network,
	platform,
	stack_descriptor::{
		FETCH_TIMEOUT, StackProjection, changed_pinned_fields, fetch, pointer_of, read_stack,
		refused, unix_now,
	},
	terminal_ui::qr_rows,
};

/// Where `publish` writes the descriptor under its output directory.
const WELL_KNOWN: &str = ".well-known/trinity-stack.json";

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
	/// `sequence`, and pins the `tld`, and for a built-in stack allowed reserved
	/// values its genesis hashes, `assets`, `contracts`, `dotns` and `coinage`.
	/// Refuses an expired copy.
	Bundle {
		/// A link or a URL.
		stack: String,
		/// The public key the descriptor must be signed with.
		#[arg(long)]
		key: Option<String>,
		/// File to write the bundled copy to.
		#[arg(long)]
		out: PathBuf,
		/// Leave `--out` untouched when the fetched copy differs from it only in
		/// fields truapi-host does not read.
		#[arg(long)]
		skip_same_projection: bool,
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
		StackCommand::Bundle { stack, key, out, skip_same_projection } => {
			bundle(&stack, key.as_deref(), &out, skip_same_projection).await
		},
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
	platform::atomic_write(path, contents)?;
	Ok(())
}

async fn verify(stack: &str, key: Option<&str>) -> Result<()> {
	let (pointer, bytes) = read_stack(stack, key).await?;
	let bytes = bytes.as_slice();
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

async fn bundle(
	stack: &str,
	key: Option<&str>,
	out: &Path,
	skip_same_projection: bool,
) -> Result<()> {
	let Some(pointer) = pointer_of(stack, key)?.filter(|pointer| pointer.key().is_some()) else {
		bail!("bundle needs a signed stack: a link or URL with a key");
	};
	let previous = match fs::read(out) {
		Ok(bytes) => Some(bytes),
		Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
		Err(error) => return Err(error).with_context(|| format!("read {}", out.display())),
	};
	let fetched = fetch(pointer.url().as_str(), FETCH_TIMEOUT)
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
	let descriptor = &accepted.stack.descriptor;
	if let Some(bytes) = previous.as_deref() {
		let stored_document = Document::parse(bytes).map_err(|error| {
			anyhow!(
				"not bundled: cannot read the tld of {}: {}; remove it to bundle {} on purpose",
				out.display(),
				refused(error),
				pointer.url()
			)
		})?;
		let stored_descriptor = stored_document.descriptor();
		if stored_descriptor.tld != descriptor.tld {
			bail!(
				"not bundled: {} changes the tld from {} to {}, which changes every account derived \
				 under it; remove {} to bundle the new tld on purpose",
				pointer.url(),
				stored_descriptor.tld,
				descriptor.tld,
				out.display()
			);
		}
		let changed_pinned = changed_pinned_fields(stored_descriptor, descriptor);
		if Network::has_pinned_fields_at(&pointer) && !changed_pinned.is_empty() {
			bail!(
				"not bundled: {} changes the {}, which this build pins for a stack allowed reserved \
				 values; remove {} to bundle the change on purpose",
				pointer.url(),
				changed_pinned.join(", "),
				out.display()
			);
		}
		let is_current = skip_same_projection &&
			open(bytes, &pointer).is_ok() &&
			StackProjection::of(stored_descriptor) == StackProjection::of(descriptor) &&
			stored_descriptor.expires_at == descriptor.expires_at &&
			stored_descriptor.valid_until == descriptor.valid_until;
		if is_current {
			println!(
				"{} is current: {} sequence {} changes no field truapi-host reads",
				out.display(),
				descriptor.id,
				descriptor.sequence
			);
			return Ok(());
		}
	}
	write_atomically(out, &accepted.document)?;
	println!("wrote {}, {} sequence {}", out.display(), descriptor.id, descriptor.sequence);
	Ok(())
}
