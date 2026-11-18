use crate::Pointer;
use std::collections::BTreeMap;

const KNOWN_STACKS: &str = include_str!("../known-stacks.json");

fn links() -> BTreeMap<&'static str, &'static str> {
	serde_json::from_str(KNOWN_STACKS).expect("invalid known-stacks.json")
}

/// The pointer of a known Trinity stack, by the name `known-stacks.json` lists it under, or `None`
/// for any other name.
pub fn known_stack(name: &str) -> Option<Pointer> {
	links()
		.get(name)
		.map(|link| Pointer::from_link(link).expect("invalid known stack link"))
}

/// Every known Trinity stack as its name and link, in name order.
///
/// Each link carries the stack's descriptor URL and signing key, as [`Pointer::from_link`] reads
/// them.
pub fn known_stack_links() -> impl Iterator<Item = (&'static str, &'static str)> {
	links().into_iter()
}

#[cfg(test)]
mod tests;
