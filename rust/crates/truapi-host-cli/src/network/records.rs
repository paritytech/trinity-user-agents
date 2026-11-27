//! Where the CLI keeps the last descriptor the core verified for a stack, so
//! the next command does not go back to the bundled copy or wait on the
//! descriptor's server.

use std::{fs, io, io::Write, path::PathBuf};

use truapi::{
	latest::GenericError,
	platform::{ReplaceCheck, StackRecords},
};

/// The file a stack's record is kept in, in its state directory.
pub const RECORD_FILE: &str = "stack-record.json";

/// One stack's descriptor record, kept in a file in its state directory.
pub struct StackRecordFile {
	path: PathBuf,
}

impl StackRecordFile {
	/// The record kept in `state_directory`, the stack's own directory.
	pub fn new(state_directory: PathBuf) -> Self {
		Self { path: state_directory.join(RECORD_FILE) }
	}

	fn read(&self) -> io::Result<Option<Vec<u8>>> {
		match fs::read(&self.path) {
			Ok(record) => Ok(Some(record)),
			Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
			Err(error) => Err(error),
		}
	}

	/// Writes `record` to a temporary file beside the record and renames it
	/// over the record, so a reader never sees half of one.
	fn replace(&self, record: &[u8]) -> io::Result<()> {
		let directory = self.path.parent().expect("a record file is in a directory");
		fs::create_dir_all(directory)?;
		let mut file = tempfile::NamedTempFile::new_in(directory)?;
		file.write_all(record)?;
		file.as_file().sync_all()?;
		file.persist(&self.path)?;
		Ok(())
	}
}

#[async_trait::async_trait]
impl StackRecords for StackRecordFile {
	async fn read_stack_record(&self, _stack_id: &str) -> Result<Option<Vec<u8>>, GenericError> {
		self.read().map_err(|error| failed(&self.path, error))
	}

	/// Reads the record before it writes. The two steps are not one
	/// transaction, so a second process writing in between can still be
	/// overwritten.
	async fn write_stack_record(
		&self,
		_stack_id: &str,
		record: Vec<u8>,
		check: ReplaceCheck,
	) -> Result<(), GenericError> {
		let stored = self.read().map_err(|error| failed(&self.path, error))?;
		if stored.is_some_and(|stored| !check.replaces(&stored)) {
			return Ok(());
		}
		self.replace(&record).map_err(|error| failed(&self.path, error))
	}
}

fn failed(path: &std::path::Path, error: impl std::fmt::Display) -> GenericError {
	format!("{}: {error}", path.display()).into()
}

#[cfg(test)]
mod tests;
