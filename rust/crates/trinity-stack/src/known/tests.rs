use super::*;

/// `known_stack` panics on a link that does not read as a pointer.
#[test]
fn test_known_stack_links_parse() {
	let invalid: Vec<_> = known_stack_links()
		.filter(|(_, link)| Pointer::from_link(link).is_err())
		.collect();
	assert_eq!(invalid, Vec::new());
}

#[test]
fn test_unknown_stack() {
	assert_eq!(known_stack("not-a-stack"), None);
}
