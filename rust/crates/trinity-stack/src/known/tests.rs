use super::*;

/// A known stack without a key would let whoever serves its URL change what it describes.
#[test]
fn test_known_stacks_signed() {
	let unsigned: Vec<_> = known_stack_links()
		.filter(|(name, _)| known_stack(name).expect("unknown name").key().is_none())
		.collect();
	assert_eq!(unsigned, Vec::new());
}

#[test]
fn test_known_stack_lookup() {
	let (name, link) = known_stack_links().next().expect("no known stacks");
	assert_eq!(known_stack(name), Some(Pointer::from_link(link).expect("invalid link")));
	assert_eq!(known_stack("not-a-stack"), None);
}
