//! Serving a published product's archive once the host has downloaded it:
//! the core lists the files that may answer a request, in order, with the
//! MIME type each is served with, and the host serves the first one it has.

/// The page a directory is served by.
const INDEX: &str = "index.html";

/// A file that may answer a request into an archive.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ArchiveFile {
	/// Its `/`-separated path.
	pub path: String,
	/// The MIME type it is served with.
	pub mime: String,
}

/// `root` joined with `rest`.
fn join(root: &str, rest: &str) -> String {
	match root.trim_end_matches('/') {
		"" => rest.to_string(),
		root => format!("{root}/{rest}"),
	}
}

/// The files in the archive at `root` that may answer `request_path`, in
/// order: the host serves the first that is a regular file, so a directory
/// is skipped for its `index.html`, and answers a 404 when none is.
///
/// `root` itself comes first, as a single-file archive answers everything.
/// Then the root's `index.html` for an empty path, the file itself, a
/// directory's `index.html` for a last segment without an extension, and
/// for a main-frame navigation the root's `index.html`, so an App's own
/// routes load it. A path that climbs out with `..` gets `root` alone.
#[uniffi::export]
pub fn archive_files(root: String, request_path: String, is_main_frame: bool) -> Vec<ArchiveFile> {
	archive_paths(&root, &request_path, is_main_frame)
		.into_iter()
		.map(|path| ArchiveFile { mime: mime_for(&path), path })
		.collect()
}

fn archive_paths(root: &str, request_path: &str, is_main_frame: bool) -> Vec<String> {
	let mut paths = vec![root.to_string()];
	let path = request_path.trim_matches('/');
	let index = join(root, INDEX);
	if path.is_empty() {
		paths.push(index);
		return paths;
	}
	if path.split('/').any(|segment| segment == "..") {
		return paths;
	}
	let exact = join(root, path);
	let last = path.rsplit('/').next().unwrap_or(path);
	let directory_index = (!last.contains('.')).then(|| join(&exact, INDEX));
	paths.push(exact);
	paths.extend(directory_index);
	if is_main_frame {
		paths.push(index);
	}
	paths
}

/// The MIME type a file at `path` is served with, by its extension in any
/// case; `application/octet-stream` for one `mime_guess` does not know.
fn mime_for(path: &str) -> String {
	mime_guess::from_path(path).first_or_octet_stream().to_string()
}

#[cfg(test)]
mod tests;
