//! Storage for secrets of the CLI signing host: hex values under hex keys in
//! one JSON file in its state directory. It is a development host, so the
//! file is not encrypted; the core only relies on it keeping what it stores.

use std::{
	collections::BTreeMap,
	io::ErrorKind,
	path::{Path, PathBuf},
	sync::Mutex,
};

use truapi::{encrypted_storage::EncryptedStorage, latest::GenericError, platform::async_trait};
use zeroize::Zeroizing;

use crate::platform::atomic_write;

/// The values stored in one file, read and replaced whole on every call.
pub struct CliCleartextStorage {
	path: PathBuf,
	writing: Mutex<()>,
}

impl CliCleartextStorage {
	/// The storage kept at `path`.
	pub fn new(path: PathBuf) -> Self {
		Self { path, writing: Mutex::new(()) }
	}
}

#[async_trait]
impl EncryptedStorage for CliCleartextStorage {
	async fn read_encrypted(&self, key: &[u8]) -> Result<Option<Zeroizing<Vec<u8>>>, GenericError> {
		let values = read_values(&self.path)?;
		let Some(value) = values.get(&hex::encode(key)) else {
			return Ok(None);
		};
		let bytes =
			hex::decode(value).map_err(|error| file_error("decoding", &self.path, error))?;
		Ok(Some(Zeroizing::new(bytes)))
	}

	async fn write_encrypted(&self, key: &[u8], value: &[u8]) -> Result<(), GenericError> {
		let _writing = self.writing.lock().expect("cleartext storage mutex poisoned");
		let mut values = read_values(&self.path)?;
		values.insert(hex::encode(key), hex::encode(value));
		let text = serde_json::to_string_pretty(&values)
			.map_err(|error| file_error("encoding", &self.path, error))?;
		atomic_write(&self.path, text.as_bytes())
			.map_err(|error| file_error("writing", &self.path, error))
	}
}

/// Every stored value by hex key, or none before the first write.
fn read_values(path: &Path) -> Result<BTreeMap<String, String>, GenericError> {
	let text = match std::fs::read_to_string(path) {
		Ok(text) => text,
		Err(error) if error.kind() == ErrorKind::NotFound => return Ok(BTreeMap::new()),
		Err(error) => return Err(file_error("reading", path, error)),
	};
	serde_json::from_str(&text).map_err(|error| file_error("decoding", path, error))
}

/// A failed file operation, naming the file.
fn file_error(action: &str, path: &Path, error: impl std::fmt::Display) -> GenericError {
	GenericError { reason: format!("{action} cleartext storage file {}: {error}", path.display()) }
}

#[cfg(test)]
mod tests {
	use super::*;

	/// A receive key's secret is written once and read back when its cheque
	/// arrives, possibly after a restart, so values must survive a new store
	/// over the same file, and a key never written reads as absent.
	#[tokio::test]
	async fn a_written_value_reads_back_from_a_new_store_and_others_are_absent() {
		let directory = tempfile::tempdir().expect("temporary directory");
		let path = directory.path().join("cleartext-storage.json");
		CliCleartextStorage::new(path.clone())
			.write_encrypted(b"key", b"secret")
			.await
			.unwrap();

		let reopened = CliCleartextStorage::new(path);
		let stored = reopened.read_encrypted(b"key").await.unwrap();
		let absent = reopened.read_encrypted(b"other").await.unwrap();

		assert_eq!(stored.as_deref().map(Vec::as_slice), Some(b"secret".as_slice()));
		assert_eq!(absent, None);
	}
}
