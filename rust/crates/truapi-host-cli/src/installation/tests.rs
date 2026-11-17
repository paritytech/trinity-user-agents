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
