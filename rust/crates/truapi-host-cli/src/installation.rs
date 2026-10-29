//! Installation id for the CLI signing host: 32 random bytes kept in a file
//! in its state directory, created on first use. The CLI registers it
//! nowhere, so recovery cannot find keys derived under it.

use std::{
	io::{ErrorKind, Write},
	path::{Path, PathBuf},
};

use truapi::{
	installation::{InstallationId, InstallationIdProvider},
	latest::GenericError,
	platform::async_trait,
};

/// The installation id stored at one path.
pub struct CliInstallation {
	path: PathBuf,
}

impl CliInstallation {
	/// The installation stored at `path`.
	pub fn new(path: PathBuf) -> Self {
		Self { path }
	}

	/// The stored id, or `None` before the first one is written.
	fn read_stored(&self) -> Result<Option<InstallationId>, GenericError> {
		match std::fs::read(&self.path) {
			Ok(bytes) => InstallationId::try_from(bytes).map(Some),
			Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
			Err(error) => Err(file_error("reading", &self.path, error)),
		}
	}

	/// Stores a fresh random id and returns it, or returns the one another
	/// call stored first.
	fn create_unless_stored(&self) -> Result<InstallationId, GenericError> {
		let fresh = random_installation()?;
		match self.write_new(&fresh) {
			Ok(()) => Ok(fresh),
			Err(error) if error.kind() == ErrorKind::AlreadyExists => {
				let vanished = || file_error("reading", &self.path, ErrorKind::NotFound.into());
				self.read_stored()?.ok_or_else(vanished)
			},
			Err(error) => Err(file_error("writing", &self.path, error)),
		}
	}

	/// Puts `id` in the file whole, through a temporary file beside it. Fails
	/// with `AlreadyExists` when the file is already there.
	fn write_new(&self, id: &InstallationId) -> std::io::Result<()> {
		let directory = self.path.parent().unwrap_or(Path::new("."));
		let mut file = tempfile::NamedTempFile::new_in(directory)?;
		file.write_all(&id.0)?;
		file.as_file().sync_all()?;
		file.persist_noclobber(&self.path).map_err(|failed| failed.error)?;
		Ok(())
	}
}

#[async_trait]
impl InstallationIdProvider for CliInstallation {
	async fn current_installation(&self) -> Result<InstallationId, GenericError> {
		match self.read_stored()? {
			Some(id) => Ok(id),
			None => self.create_unless_stored(),
		}
	}
}

fn random_installation() -> Result<InstallationId, GenericError> {
	let mut bytes = [0u8; 32];
	getrandom::getrandom(&mut bytes).map_err(|error| GenericError {
		reason: format!("generating a random installation id: {error}"),
	})?;
	Ok(InstallationId(bytes))
}

/// A failed file operation, naming the file.
fn file_error(action: &str, path: &Path, error: std::io::Error) -> GenericError {
	GenericError { reason: format!("{action} installation file {}: {error}", path.display()) }
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Keys are derived under the installation, so a CLI that picked a new id
	/// on a later call or a later run would allocate under a second one.
	#[tokio::test]
	async fn the_first_id_stays_the_installation() {
		let directory = tempfile::tempdir().expect("temporary directory");
		let path = directory.path().join("installation");
		let first = CliInstallation::new(path.clone()).current_installation().await;
		let again = CliInstallation::new(path.clone()).current_installation().await;

		assert_eq!(first.expect("created"), again.expect("read back"));
	}

	/// A call that loses the race to create the file finds an id already
	/// stored, and must return that one rather than its own.
	#[test]
	fn creating_over_a_stored_id_keeps_it() {
		let directory = tempfile::tempdir().expect("temporary directory");
		let installation = CliInstallation::new(directory.path().join("installation"));
		let stored = installation.create_unless_stored().expect("created");

		let kept = installation.create_unless_stored().expect("kept");

		assert_eq!(kept, stored);
	}
}
