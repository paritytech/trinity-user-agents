//! Installation id for the CLI signing host: 32 random bytes kept in a file
//! in its state directory, created on first use. The CLI registers it
//! nowhere, so recovery cannot find keys derived under it.

use std::{io::ErrorKind, path::PathBuf};

use truapi::{
	latest::GenericError,
	platform::{InstallationId, InstallationIdProvider, async_trait},
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

	/// Writes a fresh random id and returns it.
	fn create(&self) -> Result<InstallationId, GenericError> {
		let mut bytes = [0u8; 32];
		getrandom::getrandom(&mut bytes)
			.map_err(|error| GenericError { reason: format!("no randomness: {error}") })?;
		std::fs::write(&self.path, bytes)
			.map_err(|error| file_error("writing", &self.path, error))?;
		Ok(InstallationId(bytes))
	}
}

#[async_trait]
impl InstallationIdProvider for CliInstallation {
	async fn current_installation(&self) -> Result<InstallationId, GenericError> {
		match self.read_stored()? {
			Some(id) => Ok(id),
			None => self.create(),
		}
	}
}

/// A failed file operation, naming the file.
fn file_error(action: &str, path: &std::path::Path, error: std::io::Error) -> GenericError {
	GenericError { reason: format!("{action} installation file {}: {error}", path.display()) }
}
