use crate::Error;
use core::{cell::RefCell, fmt};
use serde::de::{self, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use std::collections::HashSet;

/// Parses `bytes` as I-JSON (RFC 7493): valid UTF-8 JSON with no repeated member names.
///
/// serde_json keeps the last of two equal keys and skips ignored fields unread, so a repeated
/// member would otherwise pass unnoticed. This walks every object once before anything else reads
/// the document.
pub fn parse(bytes: &[u8]) -> Result<serde_json::Value, Error> {
	let duplicate = RefCell::new(None);
	let mut deserializer = serde_json::Deserializer::from_slice(bytes);
	let walked = Walk { duplicate: &duplicate }
		.deserialize(&mut deserializer)
		.and_then(|()| deserializer.end());
	if let Some(name) = duplicate.into_inner() {
		return Err(Error::DuplicateMember(name));
	}
	walked.map_err(|error| Error::NotJson(error.to_string()))?;
	serde_json::from_slice(bytes).map_err(|error| Error::NotJson(error.to_string()))
}

#[derive(Clone, Copy)]
struct Walk<'a> {
	duplicate: &'a RefCell<Option<String>>,
}

impl<'de> DeserializeSeed<'de> for Walk<'_> {
	type Value = ();

	fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
		deserializer.deserialize_any(self)
	}
}

impl<'de> Visitor<'de> for Walk<'_> {
	type Value = ();

	fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
		formatter.write_str("a JSON value")
	}

	fn visit_bool<E>(self, _: bool) -> Result<(), E> {
		Ok(())
	}

	fn visit_i64<E>(self, _: i64) -> Result<(), E> {
		Ok(())
	}

	fn visit_u64<E>(self, _: u64) -> Result<(), E> {
		Ok(())
	}

	fn visit_f64<E>(self, _: f64) -> Result<(), E> {
		Ok(())
	}

	fn visit_str<E: de::Error>(self, value: &str) -> Result<(), E> {
		refuse_noncharacters(value)
	}

	fn visit_unit<E>(self) -> Result<(), E> {
		Ok(())
	}

	fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
		while seq.next_element_seed(self)?.is_some() {}
		Ok(())
	}

	fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
		let mut seen = HashSet::new();
		while let Some(name) = map.next_key::<String>()? {
			refuse_noncharacters(&name)?;
			if !seen.insert(name.clone()) {
				*self.duplicate.borrow_mut() = Some(name);
				return Err(de::Error::custom("duplicate member"));
			}
			map.next_value_seed(self)?;
		}
		Ok(())
	}
}

/// I-JSON forbids Unicode noncharacters in names and strings: U+FDD0 to U+FDEF, and the last two
/// code points of every plane.
fn refuse_noncharacters<E: de::Error>(text: &str) -> Result<(), E> {
	match text.chars().find(|&character| {
		let code = u32::from(character);
		(0xfdd0..=0xfdef).contains(&code) || code & 0xfffe == 0xfffe
	}) {
		Some(character) => Err(E::custom(format!("noncharacter U+{:04X}", u32::from(character)))),
		None => Ok(()),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn refuses_a_duplicate_member_at_any_depth() {
		assert_eq!(parse(br#"{"a":1,"a":2}"#), Err(Error::DuplicateMember("a".into())));
		assert_eq!(
			parse(br#"{"x":[{"b":{"c":1,"c":1}}]}"#),
			Err(Error::DuplicateMember("c".into()))
		);
	}

	#[test]
	fn refuses_what_is_not_json() {
		assert_eq!(parse(b"{} trailing").map_err(|error| error.code()), Err("not-json"));
		assert_eq!(parse(b"\"\xff\"").map_err(|error| error.code()), Err("not-json"));
		assert_eq!(parse(br#""\ud800""#).map_err(|error| error.code()), Err("not-json"));
		for noncharacter in [r#"{"a":"￿"}"#, "{\"\u{fdd0}\":1}", "[\"\u{10fffe}\"]"] {
			assert_eq!(
				parse(noncharacter.as_bytes()).map_err(|error| error.code()),
				Err("not-json"),
				"{noncharacter:?}"
			);
		}
		assert!(parse("[\"\u{fdcf}\u{fffd}\"]".as_bytes()).is_ok());
	}
}
