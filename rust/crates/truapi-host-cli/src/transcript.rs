//! JSON-lines transcripts the CLI hosts write, one object per line,
//! so a battery can assert what a host saw rather than only what the product
//! was told.

use std::{fs::OpenOptions, io::Write, path::PathBuf};

/// One host's transcript, or none when no path was given.
pub struct Transcript {
	path: Option<PathBuf>,
	/// The host writing it, named in warnings.
	host: &'static str,
}

impl Transcript {
	/// `host`'s transcript at the path environment variable `variable`
	/// names, if it names one.
	pub fn from_env(variable: &str, host: &'static str) -> Self {
		Self::new(std::env::var_os(variable).map(PathBuf::from), host)
	}

	/// `host`'s transcript at the path environment variable `variable`
	/// names, appended to as it stands: for a writer joining a transcript
	/// another one started.
	pub fn appending_from_env(variable: &str, host: &'static str) -> Self {
		Self { path: std::env::var_os(variable).map(PathBuf::from), host }
	}

	/// `host`'s transcript at `path`, if given. The file is truncated so a
	/// run never reads an earlier run's lines as its own.
	pub fn new(path: Option<PathBuf>, host: &'static str) -> Self {
		if let Some(path) = path.as_ref() &&
			let Err(error) = std::fs::write(path, b"")
		{
			tracing::warn!(?path, %error, host, "transcript could not be truncated");
		}
		Self { path, host }
	}

	/// Append `line`, if a transcript is configured.
	///
	/// The line and its terminator go out in a single write: several writes
	/// would let a concurrent connection's line land in the middle of this
	/// one.
	pub fn record(&self, line: serde_json::Value) {
		let Some(path) = self.path.as_ref() else {
			return;
		};
		let appended = OpenOptions::new()
			.create(true)
			.append(true)
			.open(path)
			.and_then(|mut file| file.write_all(format!("{line}\n").as_bytes()));
		if let Err(error) = appended {
			tracing::warn!(?path, %error, host = self.host, "transcript could not be appended to");
		}
	}
}
