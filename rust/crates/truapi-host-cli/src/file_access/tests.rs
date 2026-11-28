use std::{fs, os::unix::fs::PermissionsExt};

use super::*;

/// A private file holds keys, so it must never be readable by another user,
/// whatever the umask would have allowed, and the directories created for it
/// must not let another user list or open what they hold.
#[test]
fn a_private_file_and_its_new_directories_are_readable_by_the_owner_only() {
	let root = tempfile::tempdir().expect("temporary directory");
	let directory = root.path().join("state").join("session");
	let path = directory.join("core-storage.json");

	write_atomically(&path, b"secret", FileAccess::Private).expect("write");

	let modes = [mode(&path), mode(&directory), mode(&root.path().join("state"))];
	assert_eq!(
		(fs::read(&path).expect("read"), modes),
		(b"secret".to_vec(), [0o600, 0o700, 0o700])
	);
}

/// A file that is already readable by everyone must not pass that mode on to
/// the secret that replaces it, nor to the ones written after it.
#[test]
fn replacing_a_readable_file_leaves_it_readable_by_the_owner_only() {
	let root = tempfile::tempdir().expect("temporary directory");
	let path = root.path().join("device-storage.json");
	fs::write(&path, b"old").expect("write the old file");
	fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("chmod");

	write_atomically(&path, b"new", FileAccess::Private).expect("first write");
	write_atomically(&path, b"newer", FileAccess::Private).expect("second write");

	assert_eq!((fs::read(&path).expect("read"), mode(&path)), (b"newer".to_vec(), 0o600));
}

/// A directory the user made is theirs: writing into it must not change who
/// may read it.
#[test]
fn an_existing_directory_keeps_its_permissions() {
	let root = tempfile::tempdir().expect("temporary directory");
	fs::set_permissions(root.path(), fs::Permissions::from_mode(0o755)).expect("chmod");

	write_atomically(&root.path().join("accounts.json"), b"{}", FileAccess::Private)
		.expect("write");

	assert_eq!(mode(root.path()), 0o755);
}

/// The message is all a user has to diagnose a failed state write, so it
/// names the step and both paths. The failure must also leave the target as
/// it was and no temporary file to pile up in the state directory.
#[test]
fn a_failed_rename_names_its_paths_and_leaves_the_target_alone() {
	let root = tempfile::tempdir().expect("temporary directory");
	let path = root.path().join("core-storage.json");
	fs::create_dir(&path).expect("block the target with a directory");
	fs::write(path.join("kept"), b"kept").expect("fill the directory");

	let error = write_atomically(&path, b"{}", FileAccess::Private).expect_err("rename fails");

	let WriteError::Rename { temporary, path: target, .. } = &error else {
		panic!("only the rename can fail over a directory, so the error must name it: {error}");
	};
	let temporary_name = temporary.file_name().expect("a file name").to_string_lossy();
	let entries: Vec<String> = fs::read_dir(root.path())
		.expect("list")
		.map(|entry| entry.expect("entry").file_name().to_string_lossy().into_owned())
		.collect();
	assert_eq!(
		(
			temporary.parent(),
			temporary_name.starts_with(".core-storage.json."),
			target,
			error.to_string(),
			fs::read(path.join("kept")).expect("read the kept file"),
			entries,
		),
		(
			Some(root.path()),
			true,
			&path,
			format!("rename {} over {}", temporary.display(), path.display()),
			b"kept".to_vec(),
			vec!["core-storage.json".to_string()],
		)
	);
}

/// Callers undo their own work on an error, such as a session promotion
/// whose pointer write fails, so an error must mean the file was not
/// replaced. Once the rename is done, a directory that cannot be opened to
/// sync it is no reason to report one. The owner can create and rename files
/// in this directory but not open it. Root can open it anyway, and then the
/// sync succeeds and the assertion still holds.
#[test]
fn a_directory_that_cannot_be_synced_still_counts_as_written() {
	let root = tempfile::tempdir().expect("temporary directory");
	let directory = root.path().join("unlistable");
	fs::create_dir(&directory).expect("create the directory");
	let path = directory.join("current-session");
	fs::set_permissions(&directory, fs::Permissions::from_mode(0o300)).expect("chmod");

	let written = write_atomically(&path, b"promoted\n", FileAccess::Private);

	let contents = fs::read(&path);
	fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).expect("restore");
	assert_eq!(
		(written.is_ok(), contents.expect("read the replaced file")),
		(true, b"promoted\n".to_vec())
	);
}
