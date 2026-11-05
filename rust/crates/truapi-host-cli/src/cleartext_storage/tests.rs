use super::*;

/// A secret is read back after a restart, so values must survive a new
/// store over the same file, and a key never written reads as absent.
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

/// A deleted key reads as absent, the others stay, and deleting a key
/// that was never written succeeds.
#[tokio::test]
async fn a_deleted_value_reads_as_absent_and_others_stay() {
	let directory = tempfile::tempdir().expect("temporary directory");
	let storage = CliCleartextStorage::new(directory.path().join("cleartext-storage.json"));
	storage.write_encrypted(b"gone", b"one").await.unwrap();
	storage.write_encrypted(b"kept", b"two").await.unwrap();

	storage.delete_encrypted(b"gone").await.unwrap();
	storage.delete_encrypted(b"never").await.unwrap();

	assert_eq!(storage.read_encrypted(b"gone").await.unwrap(), None);
	let kept = storage.read_encrypted(b"kept").await.unwrap();
	assert_eq!(kept.as_deref().map(Vec::as_slice), Some(b"two".as_slice()));
}

/// The file holds receive key secrets, so other users must not read it.
#[cfg(unix)]
#[tokio::test]
async fn the_file_is_readable_by_its_owner_only() {
	use std::os::unix::fs::PermissionsExt;

	let directory = tempfile::tempdir().expect("temporary directory");
	let path = directory.path().join("cleartext-storage.json");
	CliCleartextStorage::new(path.clone())
		.write_encrypted(b"key", b"secret")
		.await
		.unwrap();

	let mode = std::fs::metadata(&path).unwrap().permissions().mode();
	assert_eq!(mode & 0o777, 0o600);
}
