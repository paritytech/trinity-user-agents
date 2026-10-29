use crate::Error;
use core::{cell::RefCell, fmt};
use serde::de::{self, DeserializeSeed, Deserializer, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

/// Parses `bytes` as I-JSON (RFC 7493): valid UTF-8 JSON with no repeated member names and no
/// Unicode noncharacters, anywhere in the document, ignored fields included.
///
/// serde_json keeps the last of two equal keys and skips ignored fields unread, so the checks
/// happen in the one pass that builds the document.
pub fn parse(bytes: &[u8]) -> Result<Value, Error> {
	let duplicate = RefCell::new(None);
	let mut deserializer = serde_json::Deserializer::from_slice(bytes);
	let parsed = Walk { duplicate: &duplicate }
		.deserialize(&mut deserializer)
		.and_then(|value| deserializer.end().map(|()| value));
	if let Some(name) = duplicate.into_inner() {
		return Err(Error::DuplicateMember(name));
	}
	parsed.map_err(|error| Error::NotJson(error.to_string()))
}

/// Builds a `Value` while checking it. A duplicate is recorded aside so it keeps its own error.
#[derive(Clone, Copy)]
struct Walk<'a> {
	duplicate: &'a RefCell<Option<String>>,
}

impl<'de> DeserializeSeed<'de> for Walk<'_> {
	type Value = Value;

	fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
		deserializer.deserialize_any(self)
	}
}

impl<'de> Visitor<'de> for Walk<'_> {
	type Value = Value;

	fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
		formatter.write_str("a JSON value")
	}

	fn visit_bool<E>(self, value: bool) -> Result<Value, E> {
		Ok(Value::Bool(value))
	}

	fn visit_i64<E>(self, value: i64) -> Result<Value, E> {
		Ok(Value::Number(value.into()))
	}

	fn visit_u64<E>(self, value: u64) -> Result<Value, E> {
		Ok(Value::Number(value.into()))
	}

	fn visit_f64<E: de::Error>(self, value: f64) -> Result<Value, E> {
		Number::from_f64(value)
			.map(Value::Number)
			.ok_or_else(|| E::custom("number is not finite"))
	}

	fn visit_str<E: de::Error>(self, value: &str) -> Result<Value, E> {
		refuse_noncharacters(value)?;
		Ok(Value::String(value.to_owned()))
	}

	fn visit_unit<E>(self) -> Result<Value, E> {
		Ok(Value::Null)
	}

	fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
		let mut items = Vec::new();
		while let Some(item) = seq.next_element_seed(self)? {
			items.push(item);
		}
		Ok(Value::Array(items))
	}

	fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
		let mut members = Map::new();
		while let Some(name) = map.next_key::<String>()? {
			refuse_noncharacters(&name)?;
			if members.contains_key(&name) {
				*self.duplicate.borrow_mut() = Some(name);
				return Err(de::Error::custom("duplicate member"));
			}
			let value = map.next_value_seed(self)?;
			members.insert(name, value);
		}
		Ok(Value::Object(members))
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
