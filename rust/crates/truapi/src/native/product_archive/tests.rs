use std::collections::BTreeSet;

use super::*;

/// An archive's files by path; directories are implied by their contents.
struct Archive(BTreeSet<String>);

impl Archive {
	fn new(files: &[&str]) -> Self {
		Self(files.iter().map(|path| path.to_string()).collect())
	}
}

impl ArchiveReader for Archive {
	fn exists(&self, path: String) -> bool {
		self.0.contains(&path)
	}
}

#[test]
fn an_archive_answers_the_file_a_directory_index_or_the_app_for_its_routes() {
	let files = || {
		Box::new(Archive::new(&[
			"app/index.html",
			"app/main.js",
			"app/docs/index.html",
			"app/v1.2/index.html",
		]))
	};
	let cases = [
		("/", false, Some(("app/index.html", "text/html"))),
		("/main.js", false, Some(("app/main.js", "text/javascript"))),
		("/docs", false, Some(("app/docs/index.html", "text/html"))),
		("/v1.2/docs", false, None),
		("/settings", true, Some(("app/index.html", "text/html"))),
		("/settings", false, None),
		("/missing.js", true, Some(("app/index.html", "text/html"))),
		("/../secret", true, None),
	];
	for (path, main, expected) in cases {
		let found = archive_file("app".to_string(), path.to_string(), main, files());
		let expected = expected
			.map(|(path, mime)| ArchiveFile { path: path.to_string(), mime: mime.to_string() });
		assert_eq!(found, expected, "{path}");
	}
}

#[test]
fn a_single_file_archive_answers_every_path() {
	let found = archive_file(
		"cid".to_string(),
		"/any/route".to_string(),
		false,
		Box::new(Archive::new(&["cid"])),
	);
	assert_eq!(
		found,
		Some(ArchiveFile { path: "cid".to_string(), mime: "application/octet-stream".to_string() })
	);
}

#[test]
fn mime_types_go_by_extension() {
	let cases = [
		("a/index.HTML", "text/html"),
		("main.mjs", "text/javascript"),
		("x.wasm", "application/wasm"),
		("logo.svg", "image/svg+xml"),
		("font.woff2", "font/woff2"),
		("app.js.map", "application/json"),
		("site.webmanifest", "application/manifest+json"),
		("feed.xml", "application/xml"),
		("terms.PDF", "application/pdf"),
		("robots.txt", "text/plain"),
		("rates.csv", "text/csv"),
		("hero.avif", "image/avif"),
		("favicon.ico", "image/x-icon"),
		("intro.mp4", "video/mp4"),
		("loop.webm", "video/webm"),
		("chime.mp3", "audio/mpeg"),
		("chime.ogg", "audio/ogg"),
		("chime.wav", "audio/wav"),
		("font.ttf", "font/ttf"),
		("font.otf", "font/otf"),
		("font.eot", "application/vnd.ms-fontobject"),
		("README", "application/octet-stream"),
		("archive.zip", "application/octet-stream"),
		("a.b/file", "application/octet-stream"),
	];
	for (path, mime) in cases {
		assert_eq!(mime_for(path), mime, "{path}");
	}
}
