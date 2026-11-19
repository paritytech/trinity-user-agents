//! The repository's `stacks/` copies, which host builds bundle, against the crate's signed known
//! stacks. `scripts/update-stacks.sh` writes both copies of each signed stack from
//! `known-stacks.json`; run it when these tests fail after a change to that file.

use std::{collections::BTreeMap, fs, path::PathBuf};
use trinity_stack::{known_stack, known_stack_links, open};

/// Each file in `stacks/` with `extension`, by stack name.
fn find_stack_files(extension: &str) -> BTreeMap<String, PathBuf> {
	fs::read_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../stacks"))
		.expect("stacks/ is unreadable")
		.map(|entry| entry.expect("stacks/ is unreadable").path())
		.filter(|path| path.extension().is_some_and(|found| found == extension))
		.map(|path| {
			let name = path.file_stem().expect("no file name").to_string_lossy().into_owned();
			(name, path)
		})
		.collect()
}

/// The known stacks whose link carries a key, the only ones a host build bundles.
fn find_signed_stack_links() -> impl Iterator<Item = (&'static str, &'static str)> {
	known_stack_links()
		.filter(|(name, _)| known_stack(name).expect("unknown stack").key().is_some())
}

/// A host build bundles `stacks/<name>.link`, so it must carry the link the crate lists.
#[test]
fn test_stack_links() {
	let links: BTreeMap<String, String> = find_stack_files("link")
		.into_iter()
		.map(|(name, path)| (name, fs::read_to_string(path).expect("stack link is unreadable")))
		.collect();
	let expected: BTreeMap<String, String> = find_signed_stack_links()
		.map(|(name, link)| (name.to_owned(), format!("{link}\n")))
		.collect();
	assert_eq!(links, expected);
}

/// A host build opens `stacks/<name>.json` under its known stack's key, so each copy must verify.
#[test]
fn test_stack_copy_signatures() {
	let copies = find_stack_files("json");
	let names: Vec<_> = copies.keys().map(String::as_str).collect();
	let known: Vec<_> = find_signed_stack_links().map(|(name, _)| name).collect();
	assert_eq!(names, known);
	for (name, path) in &copies {
		let document = fs::read(path).expect("stack copy is unreadable");
		let pointer = known_stack(name).expect("unknown stack");
		open(&document, &pointer).expect("stack copy does not verify");
	}
}
