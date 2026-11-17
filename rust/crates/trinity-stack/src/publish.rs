//! The publisher's side: a model rendered as a descriptor whose `sequence` rises only when its
//! content changes, so every publisher of a stack agrees on it.

use crate::{Error, VERSION, content_hash, format, ijson};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What a publisher keeps between publishes. Keep it outside anything a wipe deletes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "js", derive(tsify::Tsify))]
#[serde(rename_all = "camelCase")]
pub struct PublishState {
	/// The `sequence` last published.
	pub sequence: u32,
	/// The 0x-prefixed [`content_hash`] of what was published.
	pub content_hash: String,
}

/// A rendered descriptor and the state to keep for the next publish.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "js", derive(tsify::Tsify))]
pub struct Rendered {
	/// The plain descriptor in its canonical form, ready to serve or to [`sign`](crate::sign).
	#[serde(serialize_with = "format::serialize_bytes")]
	#[cfg_attr(feature = "js", tsify(type = "Uint8Array"))]
	pub plain: Vec<u8>,
	/// The state to pass back as `previous` next time.
	pub state: PublishState,
}

/// Renders a model, a descriptor without `$v`, `sequence` and `generation`, as a checked `$v: 1`
/// descriptor in its canonical form. A model's own values for those three fields are ignored.
///
/// `sequence` stays the same while the content matches `previous`, and otherwise rises to the
/// larger of the previous one plus one and `now_unix_seconds`, so a publisher with no saved state
/// still never goes backwards.
pub fn render(
	model: &[u8],
	generation: &str,
	previous: Option<&PublishState>,
	now_unix_seconds: u64,
) -> Result<Rendered, Error> {
	let Value::Object(mut model) = ijson::parse(model)? else {
		return Err(Error::Malformed("a model is a JSON object".into()));
	};
	model.insert("$v".into(), VERSION.into());
	model.insert("generation".into(), generation.into());
	model.insert("sequence".into(), 0.into());
	let content_hash = format::to_hex(&content_hash(&to_bytes(&model))?);
	let sequence = match previous {
		Some(previous) if previous.content_hash == content_hash => u64::from(previous.sequence),
		Some(previous) => (u64::from(previous.sequence) + 1).max(now_unix_seconds),
		None => now_unix_seconds,
	};
	model.insert("sequence".into(), sequence.into());
	let plain = crate::canonical(&to_bytes(&model))?;
	let sequence = u32::try_from(sequence).expect("canonical refuses a sequence above u32");
	Ok(Rendered { plain, state: PublishState { sequence, content_hash } })
}

fn to_bytes(model: &serde_json::Map<String, Value>) -> Vec<u8> {
	serde_json::to_vec(model).expect("a JSON value always serialises")
}

#[cfg(test)]
mod tests;
