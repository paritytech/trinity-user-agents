//! What tests read back about the files and directories the CLI writes.

use std::{fs, os::unix::fs::PermissionsExt, path::Path};

/// The permission bits of `path`, such as `0o600`, without its file type.
///
/// # Panics
///
/// When `path` cannot be read, which a test treats as a failure.
pub fn mode(path: &Path) -> u32 {
	let metadata = fs::metadata(path)
		.unwrap_or_else(|error| panic!("cannot read the mode of {}: {error}", path.display()));
	metadata.permissions().mode() & 0o777
}
