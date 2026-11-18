//! Serving a published product's archive once the host has downloaded it:
//! the host checks files through an [`ArchiveReader`], and the core decides
//! which file answers a request and the MIME type it is served with.

/// The page a directory is served by.
const INDEX: &str = "index.html";

/// Read access to a downloaded archive's files by `/`-separated path.
#[uniffi::export(callback_interface)]
pub trait ArchiveReader: Send + Sync {
	/// Whether a regular file is at `path`: `false` for a directory, so a
	/// directory is served by its `index.html` instead.
	fn exists(&self, path: String) -> bool;
}

/// The file that answers a request into an archive.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ArchiveFile {
	/// Its path, for the reader.
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

/// The file in the archive at `root` that answers `request_path`.
///
/// A `root` that is itself a file is a single-file archive and answers
/// everything. Otherwise the root's `index.html` for an empty path, the file
/// itself, a directory's `index.html` for a last segment without an
/// extension, and for a main-frame navigation the root's `index.html`, so an
/// App's own routes load it. `None` for a path that climbs out with `..`, or
/// when nothing matches, which the host answers with a 404.
#[uniffi::export]
pub fn archive_file(
	root: String,
	request_path: String,
	is_main_frame: bool,
	reader: Box<dyn ArchiveReader>,
) -> Option<ArchiveFile> {
	let path = archive_path(&root, &request_path, is_main_frame, reader.as_ref())?;
	Some(ArchiveFile { mime: mime_for(&path).to_string(), path })
}

fn archive_path(
	root: &str,
	request_path: &str,
	is_main_frame: bool,
	reader: &dyn ArchiveReader,
) -> Option<String> {
	if reader.exists(root.to_string()) {
		return Some(root.to_string());
	}
	let path = request_path.trim_matches('/');
	let index = join(root, INDEX);
	if path.is_empty() {
		return reader.exists(index.clone()).then_some(index);
	}
	if path.split('/').any(|segment| segment == "..") {
		return None;
	}
	let exact = join(root, path);
	if reader.exists(exact.clone()) {
		return Some(exact);
	}
	let last = path.rsplit('/').next().unwrap_or(path);
	if !last.contains('.') {
		let directory_index = join(&exact, INDEX);
		if reader.exists(directory_index.clone()) {
			return Some(directory_index);
		}
	}
	(is_main_frame && reader.exists(index.clone())).then_some(index)
}

/// The MIME type a file at `path` is served with, by its extension in any
/// case; `application/octet-stream` for one not listed.
fn mime_for(path: &str) -> &'static str {
	let name = path.rsplit('/').next().unwrap_or(path);
	let extension = name.rsplit_once('.').map(|(_, extension)| extension.to_ascii_lowercase());
	match extension.as_deref() {
		Some("js" | "mjs") => "text/javascript",
		Some("css") => "text/css",
		Some("html" | "htm") => "text/html",
		Some("json" | "map") => "application/json",
		Some("webmanifest") => "application/manifest+json",
		Some("wasm") => "application/wasm",
		Some("xml") => "application/xml",
		Some("pdf") => "application/pdf",
		Some("txt") => "text/plain",
		Some("csv") => "text/csv",
		Some("svg") => "image/svg+xml",
		Some("png") => "image/png",
		Some("jpg" | "jpeg") => "image/jpeg",
		Some("gif") => "image/gif",
		Some("webp") => "image/webp",
		Some("avif") => "image/avif",
		Some("ico") => "image/x-icon",
		Some("mp4") => "video/mp4",
		Some("webm") => "video/webm",
		Some("mp3") => "audio/mpeg",
		Some("ogg") => "audio/ogg",
		Some("wav") => "audio/wav",
		Some("woff") => "font/woff",
		Some("woff2") => "font/woff2",
		Some("ttf") => "font/ttf",
		Some("otf") => "font/otf",
		Some("eot") => "application/vnd.ms-fontobject",
		_ => "application/octet-stream",
	}
}

#[cfg(test)]
mod tests;
