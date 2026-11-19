use crate::Pointer;
use std::{collections::BTreeMap, sync::LazyLock};

static LINKS: LazyLock<BTreeMap<String, String>> = LazyLock::new(|| {
	serde_json::from_str(include_str!("../known-stacks.json")).expect("invalid known-stacks.json")
});

/// The pointer of a known Trinity stack, by the name `known-stacks.json` lists it under, or `None`
/// for any other name.
pub fn known_stack(name: &str) -> Option<Pointer> {
	LINKS
		.get(name)
		.map(|link| Pointer::from_link(link).expect("invalid known stack link"))
}

/// Every known Trinity stack as its name and link, in name order.
///
/// Each link carries the stack's descriptor URL and, for a signed stack, its signing key, as
/// [`Pointer::from_link`] reads them.
pub fn known_stack_links() -> impl Iterator<Item = (&'static str, &'static str)> {
	LINKS.iter().map(|(name, link)| (name.as_str(), link.as_str()))
}

#[cfg(test)]
mod tests;
