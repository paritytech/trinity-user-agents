//! The repository's `stacks/` copies, which host builds bundle, against the crate's known stacks.
//! `scripts/update-stacks.sh` writes both copies of each stack from `known-stacks.json`.

use std::{collections::BTreeMap, fs, path::PathBuf};
use trinity_stack::{known_stack, known_stack_links, open};

const OUT_OF_DATE: &str = "stacks/ differs from known-stacks.json; run scripts/update-stacks.sh";

fn stacks_dir() -> PathBuf {
	PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../stacks")
}

/// Each file in `stacks/` with `extension`, by stack name.
fn read_stacks(extension: &str) -> BTreeMap<String, Vec<u8>> {
	fs::read_dir(stacks_dir())
		.expect("stacks/ is unreadable")
		.map(|entry| entry.expect("stacks/ is unreadable").path())
		.filter(|path| path.extension().is_some_and(|found| found == extension))
		.map(|path| {
			let name = path.file_stem().expect("no file name").to_string_lossy().into_owned();
			(name, fs::read(&path).expect("stack file is unreadable"))
		})
		.collect()
}

/// A host build bundles `stacks/<name>.link`, so it must carry the link the crate lists.
#[test]
fn test_stack_links_match_known_stacks() {
	let expected: BTreeMap<String, Vec<u8>> = known_stack_links()
		.map(|(name, link)| (name.to_owned(), format!("{link}\n").into_bytes()))
		.collect();
	assert!(read_stacks("link") == expected, "{OUT_OF_DATE}");
}

/// A host build opens `stacks/<name>.json` under its known stack's key, so each copy must verify.
#[test]
fn test_stack_copies_verify() {
	let copies = read_stacks("json");
	let names: Vec<_> = copies.keys().map(String::as_str).collect();
	let known: Vec<_> = known_stack_links().map(|(name, _)| name).collect();
	assert!(names == known, "{OUT_OF_DATE}");
	for (name, document) in &copies {
		let pointer = known_stack(name).expect("unknown stack");
		open(document, &pointer).expect("stack copy does not verify");
	}
}
