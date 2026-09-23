//! Host-private, bounded Chat file custody. Paths never leave the trusted CLI UI.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use sha2::{Digest, Sha256};
use tempfile::NamedTempFile;
use truapi::latest::{
    GenericError, HostNativeChatAttachmentKind, HostNativeChatAttachmentMetadata,
};
use truapi_platform::NativeChatPickedFile;

const MAX_READ: u32 = 2_000_000;
const BLOCK_SIZE: u64 = 65_536;
const MAGIC: &[u8; 8] = b"CHATFILE";
const HEADER_SIZE: u64 = 12;
pub(crate) const DIRECTORY: &str = "chat-files";

pub(crate) fn error(reason: &'static str) -> GenericError {
    GenericError {
        reason: reason.to_string(),
    }
}

async fn blocking<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T, GenericError> + Send + 'static,
) -> Result<T, GenericError> {
    tokio::task::spawn_blocking(operation)
        .await
        .map_err(|_| error("Chat file operation interrupted"))?
}

fn opaque_id() -> Result<String, GenericError> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| error("Chat file randomness unavailable"))?;
    Ok(hex::encode(bytes))
}

fn valid_id(id: &str) -> Result<(), GenericError> {
    if id.len() != 64
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(error("Invalid Chat file handle"));
    }
    Ok(())
}

fn sync_directory(path: &Path) -> Result<(), GenericError> {
    #[cfg(unix)]
    File::open(path)
        .and_then(|file| file.sync_all())
        .map_err(|_| error("Could not synchronize Chat file directory"))?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

fn private_directory(root: &Path) -> Result<PathBuf, GenericError> {
    let directory = root.join(DIRECTORY);
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    match builder.create(&directory) {
        Ok(()) => sync_directory(root)?,
        Err(cause) if cause.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(_) => return Err(error("Could not create private Chat file storage")),
    }
    let metadata = fs::symlink_metadata(&directory)
        .map_err(|_| error("Private Chat file storage unavailable"))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(error("Invalid private Chat file storage"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
            .map_err(|_| error("Could not protect private Chat file storage"))?;
    }
    Ok(directory)
}

fn open_regular(path: &Path) -> Result<File, GenericError> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(
            (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32,
        );
    }
    let file = options
        .open(path)
        .map_err(|_| error("Could not open Chat file"))?;
    if !file
        .metadata()
        .map_err(|_| error("Could not inspect Chat file"))?
        .is_file()
    {
        return Err(error("Chat selection must be a regular file"));
    }
    Ok(file)
}

fn source_path(root: &Path, source_id: &str) -> Result<PathBuf, GenericError> {
    valid_id(source_id)?;
    Ok(private_directory(root)?.join(source_id))
}

fn data_start(size: u32) -> u64 {
    HEADER_SIZE + u64::from(size).div_ceil(BLOCK_SIZE) * 32
}

fn remove_snapshot(path: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(std::io::Error::from(std::io::ErrorKind::InvalidData));
        }
        let mut permissions = metadata.permissions();
        permissions.set_readonly(false);
        fs::set_permissions(path, permissions)?;
    }
    fs::remove_file(path)
}

// Own completed imports until the async callback actually receives its result.
// Dropping a cancelled callback or a failed multi-selection removes its imports.
struct Imports {
    directory: PathBuf,
    files: Vec<NativeChatPickedFile>,
}

impl Drop for Imports {
    fn drop(&mut self) {
        for file in &self.files {
            let _ = remove_snapshot(&self.directory.join(&file.source_id));
        }
    }
}

fn import_one(directory: &Path, path: &Path) -> Result<NativeChatPickedFile, GenericError> {
    let mut input = open_regular(path)?;
    let before = input
        .metadata()
        .map_err(|_| error("Could not inspect selected Chat file"))?;
    let size = u32::try_from(before.len())
        .map_err(|_| error("Chat files must fit in a u32 byte count"))?;
    let mut snapshot =
        NamedTempFile::new_in(directory).map_err(|_| error("Could not create Chat snapshot"))?;
    snapshot
        .seek(SeekFrom::Start(data_start(size)))
        .map_err(|_| error("Could not prepare Chat snapshot"))?;
    let mut hashes = Vec::with_capacity(u64::from(size).div_ceil(BLOCK_SIZE) as usize * 32);
    let mut buffer = vec![0_u8; BLOCK_SIZE as usize];
    let mut remaining = u64::from(size);
    while remaining > 0 {
        let count = remaining.min(BLOCK_SIZE) as usize;
        input
            .read_exact(&mut buffer[..count])
            .map_err(|_| error("Selected Chat file changed or could not be read"))?;
        snapshot
            .write_all(&buffer[..count])
            .map_err(|_| error("Could not write Chat snapshot"))?;
        hashes.extend_from_slice(&Sha256::digest(&buffer[..count]));
        remaining -= count as u64;
    }
    let after = input
        .metadata()
        .map_err(|_| error("Could not inspect selected Chat file"))?;
    if input
        .read(&mut buffer[..1])
        .map_err(|_| error("Could not read selected Chat file"))?
        != 0
        || before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
    {
        return Err(error("Selected Chat file changed while importing"));
    }
    snapshot
        .seek(SeekFrom::Start(0))
        .and_then(|_| snapshot.write_all(MAGIC))
        .and_then(|_| snapshot.write_all(&size.to_le_bytes()))
        .and_then(|_| snapshot.write_all(&hashes))
        .map_err(|_| error("Could not seal Chat snapshot"))?;
    let mut permissions = snapshot
        .as_file()
        .metadata()
        .map_err(|_| error("Could not inspect Chat snapshot"))?
        .permissions();
    permissions.set_readonly(true);
    snapshot
        .as_file()
        .set_permissions(permissions)
        .and_then(|_| snapshot.as_file().sync_all())
        .map_err(|_| error("Could not protect and synchronize Chat snapshot"))?;
    let source_id = opaque_id()?;
    snapshot
        .persist_noclobber(directory.join(&source_id))
        .map_err(|_| error("Could not publish Chat snapshot"))?;
    Ok(NativeChatPickedFile {
        source_id,
        metadata: HostNativeChatAttachmentMetadata {
            mime_type: "application/octet-stream".to_string(),
            size_bytes: size,
            kind: HostNativeChatAttachmentKind::File,
        },
    })
}

fn import_files(root: &Path, paths: Vec<PathBuf>) -> Result<Imports, GenericError> {
    let directory = private_directory(root)?;
    let mut imports = Imports {
        directory,
        files: Vec::with_capacity(paths.len()),
    };
    for path in paths {
        imports.files.push(import_one(&imports.directory, &path)?);
    }
    sync_directory(&imports.directory)?;
    Ok(imports)
}

fn read_file(
    root: &Path,
    source_id: &str,
    offset: u64,
    length: u32,
) -> Result<Vec<u8>, GenericError> {
    if length > MAX_READ {
        return Err(error("Chat file reads are limited to 2000000 bytes"));
    }
    let end = offset
        .checked_add(u64::from(length))
        .ok_or_else(|| error("Chat file range overflow"))?;
    let mut file = open_regular(&source_path(root, source_id)?)?;
    let actual = file
        .metadata()
        .map_err(|_| error("Could not inspect Chat snapshot"))?;
    let mut header = [0_u8; HEADER_SIZE as usize];
    file.read_exact(&mut header)
        .map_err(|_| error("Invalid Chat snapshot"))?;
    if &header[..8] != MAGIC || !actual.permissions().readonly() {
        return Err(error("Chat snapshot is not immutable"));
    }
    let size = u32::from_le_bytes(header[8..12].try_into().expect("fixed size field"));
    let start = data_start(size);
    if actual.len() != start + u64::from(size) || end > u64::from(size) {
        return Err(error("Chat file range or snapshot size is invalid"));
    }
    let mut result = Vec::with_capacity(length as usize);
    if length == 0 {
        return Ok(result);
    }
    let mut buffer = vec![0_u8; BLOCK_SIZE as usize];
    for block in offset / BLOCK_SIZE..=(end - 1) / BLOCK_SIZE {
        let block_start = block * BLOCK_SIZE;
        let count = (u64::from(size) - block_start).min(BLOCK_SIZE) as usize;
        let mut expected = [0_u8; 32];
        file.seek(SeekFrom::Start(HEADER_SIZE + block * 32))
            .and_then(|_| file.read_exact(&mut expected))
            .and_then(|_| file.seek(SeekFrom::Start(start + block_start)))
            .and_then(|_| file.read_exact(&mut buffer[..count]))
            .map_err(|_| error("Could not read Chat snapshot"))?;
        if Sha256::digest(&buffer[..count])[..] != expected {
            return Err(error("Chat snapshot integrity check failed"));
        }
        let from = offset.saturating_sub(block_start) as usize;
        let to = (end - block_start).min(count as u64) as usize;
        result.extend_from_slice(&buffer[from..to]);
    }
    Ok(result)
}

struct Export {
    root: PathBuf,
    destination: PathBuf,
    temporary: Option<NamedTempFile>,
    size: u64,
    written: u64,
}

impl Export {
    fn prepare(root: PathBuf, destination: PathBuf, size: u32) -> Result<Self, GenericError> {
        let name = destination
            .file_name()
            .ok_or_else(|| error("Chat export needs a file name"))?;
        let parent = destination
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let parent =
            fs::canonicalize(parent).map_err(|_| error("Chat export directory is unavailable"))?;
        let destination = parent.join(name);
        match fs::symlink_metadata(&destination) {
            Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => {}
            _ => {
                return Err(error(
                    "Chat export destination already exists or is unavailable; choose a new file name",
                ));
            }
        }
        let temporary =
            NamedTempFile::new_in(&parent).map_err(|_| error("Could not create Chat export"))?;
        Ok(Self {
            root,
            destination,
            temporary: Some(temporary),
            size: u64::from(size),
            written: 0,
        })
    }

    fn write(&mut self, offset: u64, data: &[u8]) -> Result<(), GenericError> {
        let end = offset
            .checked_add(data.len() as u64)
            .ok_or_else(|| error("Chat export range overflow"))?;
        if data.len() > MAX_READ as usize || offset != self.written || end > self.size {
            return Err(error(
                "Chat export writes must be bounded, contiguous and within the declared size",
            ));
        }
        let file = self
            .temporary
            .as_mut()
            .ok_or_else(|| error("Chat export is already published"))?;
        // A failed write may have written a prefix. Retrying the same offset
        // overwrites that prefix instead of silently appending duplicate bytes.
        file.seek(SeekFrom::Start(offset))
            .and_then(|_| file.write_all(data))
            .map_err(|_| error("Could not write Chat export"))?;
        self.written = end;
        Ok(())
    }

    fn finish(&mut self) -> Result<(), GenericError> {
        if self.written != self.size {
            return Err(error("Chat export is incomplete"));
        }
        if let Some(file) = &self.temporary {
            if file
                .as_file()
                .metadata()
                .map_err(|_| error("Could not inspect Chat export"))?
                .len()
                != self.size
            {
                return Err(error("Chat export size is invalid"));
            }
            file.as_file()
                .sync_all()
                .map_err(|_| error("Could not synchronize Chat export"))?;
        }
        if let Some(file) = self.temporary.take()
            && let Err(cause) = file.persist_noclobber(&self.destination)
        {
            self.temporary = Some(cause.file);
            return Err(error(
                "Could not publish Chat export without overwriting an existing file",
            ));
        }
        // If this sync fails, retain the published state: cancellation may only
        // remove a staging file, never the completed user destination.
        sync_directory(
            self.destination
                .parent()
                .expect("canonical destination has a parent"),
        )
    }
}

#[derive(Clone, Default)]
pub(crate) struct ChatFiles {
    exports: Arc<Mutex<HashMap<String, Export>>>,
}

impl ChatFiles {
    pub(crate) async fn import(
        root: PathBuf,
        paths: Vec<PathBuf>,
    ) -> Result<Vec<NativeChatPickedFile>, GenericError> {
        let mut imports = blocking(move || import_files(&root, paths)).await?;
        Ok(std::mem::take(&mut imports.files))
    }

    pub(crate) async fn read(
        root: PathBuf,
        source_id: String,
        offset: u64,
        length: u32,
    ) -> Result<Vec<u8>, GenericError> {
        blocking(move || read_file(&root, &source_id, offset, length)).await
    }

    pub(crate) async fn release(root: PathBuf, source_id: String) -> Result<(), GenericError> {
        blocking(move || {
            let path = source_path(&root, &source_id)?;
            match remove_snapshot(&path) {
                Ok(()) => sync_directory(path.parent().expect("snapshot has a parent")),
                Err(cause) if cause.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(_) => Err(error("Could not release Chat snapshot")),
            }
        })
        .await
    }

    pub(crate) async fn begin_export(
        &self,
        root: PathBuf,
        destination: PathBuf,
        size: u32,
    ) -> Result<String, GenericError> {
        let (id, export) =
            blocking(move || Ok((opaque_id()?, Export::prepare(root, destination, size)?))).await?;
        self.exports
            .lock()
            .map_err(|_| error("Chat exports unavailable"))?
            .insert(id.clone(), export);
        Ok(id)
    }

    pub(crate) async fn write_export(
        &self,
        root: PathBuf,
        id: String,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<(), GenericError> {
        valid_id(&id)?;
        let exports = self.exports.clone();
        blocking(move || {
            let mut exports = exports
                .lock()
                .map_err(|_| error("Chat exports unavailable"))?;
            let export = exports
                .get_mut(&id)
                .filter(|export| export.root == root)
                .ok_or_else(|| error("Unknown Chat export"))?;
            export.write(offset, &data)
        })
        .await
    }

    pub(crate) async fn finish_export(
        &self,
        root: PathBuf,
        id: String,
    ) -> Result<(), GenericError> {
        valid_id(&id)?;
        let exports = self.exports.clone();
        blocking(move || {
            let mut exports = exports
                .lock()
                .map_err(|_| error("Chat exports unavailable"))?;
            let export = exports
                .get_mut(&id)
                .filter(|export| export.root == root)
                .ok_or_else(|| error("Unknown Chat export"))?;
            export.finish()?;
            exports.remove(&id);
            Ok(())
        })
        .await
    }

    pub(crate) async fn cancel_export(
        &self,
        root: PathBuf,
        id: String,
    ) -> Result<(), GenericError> {
        valid_id(&id)?;
        let exports = self.exports.clone();
        blocking(move || {
            let mut exports = exports
                .lock()
                .map_err(|_| error("Chat exports unavailable"))?;
            if exports.get(&id).is_some_and(|export| export.root != root) {
                return Err(error("Unknown Chat export"));
            }
            exports.remove(&id);
            Ok(())
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn snapshots_survive_original_changes_and_backend_restart_until_release() {
        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("selected");
        let bytes = vec![42; BLOCK_SIZE as usize + 7];
        fs::write(&original, &bytes).unwrap();
        let selected = ChatFiles::import(root.path().to_path_buf(), vec![original.clone()])
            .await
            .unwrap();
        let source = selected[0].source_id.clone();
        fs::write(original, b"changed").unwrap();
        // Reads require no process-local registry: only the durable source ID.
        let range = ChatFiles::read(root.path().to_path_buf(), source.clone(), BLOCK_SIZE - 2, 9)
            .await
            .unwrap();
        assert_eq!(range, bytes[BLOCK_SIZE as usize - 2..]);
        assert!(
            ChatFiles::read(root.path().to_path_buf(), source.clone(), 0, MAX_READ + 1)
                .await
                .is_err()
        );
        assert!(
            ChatFiles::read(root.path().to_path_buf(), source.clone(), u64::MAX, 2)
                .await
                .is_err()
        );
        assert!(
            ChatFiles::read(
                root.path().to_path_buf(),
                source.clone(),
                bytes.len() as u64,
                1
            )
            .await
            .is_err()
        );
        assert_eq!(
            ChatFiles::read(
                root.path().to_path_buf(),
                source.clone(),
                bytes.len() as u64,
                0
            )
            .await
            .unwrap(),
            Vec::<u8>::new()
        );
        ChatFiles::release(root.path().to_path_buf(), source.clone())
            .await
            .unwrap();
        ChatFiles::release(root.path().to_path_buf(), source.clone())
            .await
            .unwrap();
        assert!(
            ChatFiles::read(root.path().to_path_buf(), source, 0, 1)
                .await
                .is_err()
        );
        assert!(
            ChatFiles::read(root.path().to_path_buf(), "../selected".to_string(), 0, 1)
                .await
                .is_err()
        );
    }

    #[test]
    fn snapshot_corruption_is_rejected_even_when_size_does_not_change() {
        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("selected");
        fs::write(&original, b"original").unwrap();
        let imports = import_files(root.path(), vec![original]).unwrap();
        let id = &imports.files[0].source_id;
        let path = source_path(root.path(), id).unwrap();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            permissions.set_mode(0o600);
        }
        #[cfg(not(unix))]
        permissions.set_readonly(false);
        fs::set_permissions(&path, permissions.clone()).unwrap();
        let mut file = OpenOptions::new().write(true).open(&path).unwrap();
        file.seek(SeekFrom::Start(data_start(8))).unwrap();
        file.write_all(b"tampered").unwrap();
        permissions.set_readonly(true);
        file.set_permissions(permissions).unwrap();
        drop(file);
        assert!(read_file(root.path(), id, 0, 8).is_err());
    }

    #[tokio::test]
    async fn export_enforces_order_size_and_no_clobber_and_cancel_keeps_completed_file() {
        let root = tempfile::tempdir().unwrap();
        let scope = root.path().to_path_buf();
        let destination = root.path().join("download");
        let store = ChatFiles::default();
        let id = store
            .begin_export(scope.clone(), destination.clone(), 4)
            .await
            .unwrap();
        assert!(
            store
                .write_export(scope.clone(), id.clone(), 1, vec![1])
                .await
                .is_err()
        );
        assert!(
            store
                .write_export(scope.clone(), id.clone(), 0, vec![0; 5])
                .await
                .is_err()
        );
        store
            .write_export(scope.clone(), id.clone(), 0, vec![1, 2])
            .await
            .unwrap();
        assert!(
            store
                .finish_export(scope.clone(), id.clone())
                .await
                .is_err()
        );
        assert!(!destination.exists());
        store
            .write_export(scope.clone(), id.clone(), 2, vec![3, 4])
            .await
            .unwrap();
        fs::write(&destination, b"existing").unwrap();
        assert!(
            store
                .finish_export(scope.clone(), id.clone())
                .await
                .is_err()
        );
        assert_eq!(fs::read(&destination).unwrap(), b"existing");
        fs::remove_file(&destination).unwrap();
        store
            .finish_export(scope.clone(), id.clone())
            .await
            .unwrap();
        store
            .cancel_export(scope.clone(), id.clone())
            .await
            .unwrap();
        store.cancel_export(scope.clone(), id).await.unwrap();
        assert_eq!(fs::read(&destination).unwrap(), [1, 2, 3, 4]);
        let partial = root.path().join("partial");
        let id = store
            .begin_export(scope.clone(), partial.clone(), 1)
            .await
            .unwrap();
        store
            .cancel_export(scope.clone(), id.clone())
            .await
            .unwrap();
        store.cancel_export(scope, id).await.unwrap();
        assert!(!partial.exists());
    }

    #[test]
    fn failed_multi_selection_does_not_retain_partial_snapshots() {
        let root = tempfile::tempdir().unwrap();
        let selected = root.path().join("selected");
        fs::write(&selected, b"data").unwrap();
        assert!(import_files(root.path(), vec![selected, root.path().join("missing")]).is_err());
        assert_eq!(
            fs::read_dir(root.path().join(DIRECTORY)).unwrap().count(),
            0
        );
    }

    #[test]
    fn oversized_native_files_are_rejected_before_importing_bytes() {
        let root = tempfile::tempdir().unwrap();
        let selected = root.path().join("oversized");
        File::create(&selected)
            .unwrap()
            .set_len(u64::from(u32::MAX) + 1)
            .unwrap();
        assert!(import_files(root.path(), vec![selected]).is_err());
        assert_eq!(
            fs::read_dir(root.path().join(DIRECTORY)).unwrap().count(),
            0
        );
    }

    #[tokio::test]
    async fn exact_read_limit_and_identity_scope_are_enforced() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let selected = first.path().join("selected");
        let bytes = vec![17; MAX_READ as usize];
        fs::write(&selected, &bytes).unwrap();
        let imported = ChatFiles::import(first.path().to_path_buf(), vec![selected])
            .await
            .unwrap();
        let source = imported[0].source_id.clone();
        assert_eq!(
            ChatFiles::read(first.path().to_path_buf(), source.clone(), 0, MAX_READ)
                .await
                .unwrap(),
            bytes
        );
        assert!(
            ChatFiles::read(second.path().to_path_buf(), source, 0, 1)
                .await
                .is_err()
        );
        let store = ChatFiles::default();
        let destination = first.path().join("download");
        let id = store
            .begin_export(first.path().to_path_buf(), destination.clone(), 0)
            .await
            .unwrap();
        assert!(
            store
                .finish_export(second.path().to_path_buf(), id.clone())
                .await
                .is_err()
        );
        assert!(
            store
                .cancel_export(second.path().to_path_buf(), id.clone())
                .await
                .is_err()
        );
        store
            .finish_export(first.path().to_path_buf(), id.clone())
            .await
            .unwrap();
        store
            .cancel_export(first.path().to_path_buf(), id)
            .await
            .unwrap();
        assert_eq!(fs::read(destination).unwrap(), Vec::<u8>::new());
    }
}
