use std::collections::BTreeSet;

use super::*;

/// The file a host serves: the first candidate that is in `files`.
fn served(files: &BTreeSet<&str>, root: &str, path: &str, main: bool) -> Option<ArchiveFile> {
	archive_files(root.to_string(), path.to_string(), main)
		.into_iter()
		.find(|file| files.contains(file.path.as_str()))
}

#[test]
fn an_archive_answers_the_file_a_directory_index_or_the_app_for_its_routes() {
	let files = BTreeSet::from([
		"app/index.html",
		"app/main.js",
		"app/docs/index.html",
		"app/v1.2/index.html",
	]);
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
		let expected = expected
			.map(|(path, mime)| ArchiveFile { path: path.to_string(), mime: mime.to_string() });
		assert_eq!(served(&files, "app", path, main), expected, "{path}");
	}
}

#[test]
fn candidates_come_in_the_order_the_host_tries_them() {
	let paths = |path: &str, main| {
		archive_files("app/".to_string(), path.to_string(), main)
			.into_iter()
			.map(|file| file.path)
			.collect::<Vec<_>>()
	};
	assert_eq!(paths("/", false), ["app/", "app/index.html"]);
	assert_eq!(paths("/docs/", true), ["app/", "app/docs", "app/docs/index.html", "app/index.html"]);
	assert_eq!(paths("/main.js", false), ["app/", "app/main.js"]);
	assert_eq!(paths("/a/../b", true), ["app/"]);
}

#[test]
fn a_single_file_archive_answers_every_path() {
	let files = BTreeSet::from(["cid"]);
	assert_eq!(
		served(&files, "cid", "/any/route", false),
		Some(ArchiveFile { path: "cid".to_string(), mime: "application/octet-stream".to_string() })
	);
}

#[test]
fn mime_types_go_by_extension() {
	let cases = [
		("a/index.HTML", "text/html"),
		("main.mjs", "application/javascript"),
		("x.wasm", "application/wasm"),
		("logo.svg", "image/svg+xml"),
		("font.woff2", "font/woff2"),
		("site.webmanifest", "application/manifest+json"),
		("terms.PDF", "application/pdf"),
		("robots.txt", "text/plain"),
		("hero.avif", "image/avif"),
		("README", "application/octet-stream"),
		("a.b/file", "application/octet-stream"),
	];
	for (path, mime) in cases {
		assert_eq!(mime_for(path), mime, "{path}");
	}
}
