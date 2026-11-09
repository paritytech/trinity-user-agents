//! What the native funding feed tells the host, and the marker it leaves so
//! the host resumes funding at its next launch.

use std::path::{Path, PathBuf};

/// The file under the core database directory whose presence says a
/// provider still works on a funding session.
const RESUME_MARKER: &str = "funding-resume";

/// What changed in the funding state a host reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FundingChangeKind {
	/// A session's record, progress, or whether it is acknowledged; the
	/// activity with it.
	Session,
	/// What a session's overlay offers.
	Options,
	/// A session's quote list.
	Quotes,
	/// Whether a provider still works on any session.
	InProgress,
}

/// The resume marker's path under `database_directory`.
pub fn resume_marker(database_directory: &Path) -> PathBuf {
	database_directory.join(RESUME_MARKER)
}

/// Leave or clear the resume marker under `database_directory`, so the host
/// starts funding at its next launch only while a provider still works on a
/// session.
pub fn write_resume_marker(database_directory: &Path, in_progress: bool) {
	let marker = resume_marker(database_directory);
	let written = if in_progress {
		std::fs::write(&marker, b"")
	} else {
		match std::fs::remove_file(&marker) {
			Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
			other => other,
		}
	};
	if let Err(error) = written {
		tracing::error!(marker = %marker.display(), %error, "funding resume marker not written");
	}
}

#[cfg(test)]
mod tests;
