use crate::logging::{CoreAttribute, CoreAttributeValue, CoreEvent};

/// What `fact` carries under `key`.
pub fn value(fact: &CoreEvent, key: &str) -> Option<CoreAttributeValue> {
	fact.attributes()
		.iter()
		.find(|attribute| attribute.key == key)
		.map(|CoreAttribute { value, .. }| value.clone())
}

/// A label attribute's value.
pub fn text(value: &str) -> Option<CoreAttributeValue> {
	Some(CoreAttributeValue::Text { value: value.to_string() })
}

/// A count attribute's value.
pub fn count(value: u64) -> Option<CoreAttributeValue> {
	Some(CoreAttributeValue::Count { value })
}

/// A condition attribute's value.
pub fn flag(value: bool) -> Option<CoreAttributeValue> {
	Some(CoreAttributeValue::Flag { value })
}

/// `fact` as one line, its marker then each attribute, to search for
/// what it must not carry.
pub fn line(fact: &CoreEvent) -> String {
	let attributes = fact
		.attributes()
		.iter()
		.map(|CoreAttribute { key, value }| format!("{key}={value}"));
	core::iter::once(fact.marker().target.to_string())
		.chain(attributes)
		.collect::<Vec<_>>()
		.join(" ")
}
