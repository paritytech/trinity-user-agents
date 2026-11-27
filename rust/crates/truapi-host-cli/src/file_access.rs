//! Files the CLI replaces whole, and the directories it creates to hold them.
//!
//! [`write_atomically`] is the one way the CLI replaces a file it keeps, and
//! [`FileAccess`] decides who may read it.

use std::{
	ffi::{OsStr, OsString},
	fs::DirBuilder,
	io::{self, Write},
	path::{Path, PathBuf},
};

use tempfile::NamedTempFile;

/// Who may read a file the CLI writes and the directories it creates for one.
///
/// On Unix the mode is set when a file or directory is created, so a file is
/// never readable by more users than this allows, even while it is written.
/// The umask can narrow the mode but never widens it. A directory that
/// already exists keeps its permissions.
#[derive(Clone, Copy)]
pub enum FileAccess {
	/// State under the CLI's state directory, which holds sessions, device
	/// identity, keys and mnemonics: files `0600`, directories `0700`.
	Private,
	/// Files written for other programs or users to read, such as a published
	/// stack descriptor a web server serves: the umask decides, as for any
	/// new file (`0666` and `0777` before it).
	Shared,
}

impl FileAccess {
	#[cfg(unix)]
	fn file_mode(self) -> u32 {
		match self {
			Self::Private => 0o600,
			Self::Shared => 0o666,
		}
	}

	#[cfg(unix)]
	fn directory_mode(self) -> u32 {
		match self {
			Self::Private => 0o700,
			Self::Shared => 0o777,
		}
	}

	fn directory_builder(self) -> DirBuilder {
		let mut builder = DirBuilder::new();
		#[cfg(unix)]
		{
			use std::os::unix::fs::DirBuilderExt;
			builder.mode(self.directory_mode());
		}
		builder
	}

	fn create_temporary(self, directory: &Path, prefix: &OsStr) -> io::Result<NamedTempFile> {
		let mut builder = tempfile::Builder::new();
		builder.prefix(prefix).suffix(".tmp");
		#[cfg(unix)]
		{
			use std::{fs::Permissions, os::unix::fs::PermissionsExt};
			builder.permissions(Permissions::from_mode(self.file_mode()));
		}
		builder.tempfile_in(directory)
	}
}

/// Creates `path` and its missing parents with the permissions of `access`.
///
/// Directories that already exist are left as they are.
pub fn create_dir_all(path: &Path, access: FileAccess) -> io::Result<()> {
	access.directory_builder().recursive(true).create(path)
}

/// Creates the directory `path` with the permissions of `access`, failing
/// with `AlreadyExists` when it is already there.
pub fn create_dir(path: &Path, access: FileAccess) -> io::Result<()> {
	access.directory_builder().create(path)
}

/// The step of [`write_atomically`] that failed.
///
/// Each variant names the path it was working on. Any of them leaves the
/// target as it was.
#[derive(derive_more::Display, derive_more::Error, Debug)]
#[must_use]
pub enum WriteError {
	/// The target path ends in no file name, such as `/` or `..`.
	#[display("{} names no file", path.display())]
	NoFileName { path: PathBuf },
	/// The target's directory or one of its parents could not be created.
	#[display("create directory {}", path.display())]
	CreateDirectory { path: PathBuf, source: io::Error },
	/// The temporary file could not be created beside the target.
	#[display("create temporary file in {}", directory.display())]
	CreateTemporary { directory: PathBuf, source: io::Error },
	/// The new contents could not be written to the temporary file.
	#[display("write temporary file {}", temporary.display())]
	Write { temporary: PathBuf, source: io::Error },
	/// The temporary file could not be synced to disk.
	#[display("sync temporary file {}", temporary.display())]
	Sync { temporary: PathBuf, source: io::Error },
	/// The temporary file could not be renamed over the target.
	#[display("rename {} over {}", temporary.display(), path.display())]
	Rename { temporary: PathBuf, path: PathBuf, source: io::Error },
}

/// Replaces the file at `path` with `bytes`, so a reader sees the old
/// contents or the new ones and never part of either.
///
/// Missing parent directories are created as [`create_dir_all`] does. The
/// bytes go to a new temporary file beside `path`, created with the mode of
/// `access`, which is synced and renamed over `path`; on Unix the directory
/// is then synced so the rename survives a crash. The replaced file's mode is
/// not kept. An error means `path` was not replaced: a failure before the
/// rename leaves it as it was and removes the temporary file, and a failure
/// to sync the directory after it is logged, since the new contents are
/// already in place.
pub fn write_atomically(path: &Path, bytes: &[u8], access: FileAccess) -> Result<(), WriteError> {
	let Some(name) = path.file_name() else {
		return Err(WriteError::NoFileName { path: path.to_path_buf() });
	};
	let directory = path.parent().filter(|parent| !parent.as_os_str().is_empty());
	let directory = directory.unwrap_or(Path::new("."));
	create_dir_all(directory, access)
		.map_err(|source| WriteError::CreateDirectory { path: directory.to_path_buf(), source })?;
	let mut prefix = OsString::from(".");
	prefix.push(name);
	prefix.push(".");
	let mut file = access.create_temporary(directory, &prefix).map_err(|source| {
		WriteError::CreateTemporary { directory: directory.to_path_buf(), source }
	})?;
	let temporary = file.path().to_path_buf();
	if let Err(source) = file.write_all(bytes) {
		return Err(WriteError::Write { temporary, source });
	}
	if let Err(source) = file.as_file().sync_all() {
		return Err(WriteError::Sync { temporary, source });
	}
	if let Err(failed) = file.persist(path) {
		return Err(WriteError::Rename {
			temporary,
			path: path.to_path_buf(),
			source: failed.error,
		});
	}
	#[cfg(unix)]
	if let Err(error) = std::fs::File::open(directory).and_then(|directory| directory.sync_all()) {
		tracing::warn!(path = %path.display(), %error, "replaced a file but could not sync its directory");
	}
	Ok(())
}

#[cfg(all(test, unix))]
mod tests;
