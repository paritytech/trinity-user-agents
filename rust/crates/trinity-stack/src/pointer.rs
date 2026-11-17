use crate::{
	Error,
	format::{self, Schemes},
};
use serde::{Serialize, Serializer};
use url::{Url, form_urlencoded};

/// The link base the Polkadot app opens.
pub const DEFAULT_LINK_BASE: &str = "polkadotapp://stack";

/// Where a stack's descriptor lives, and the key that signs it when it is signed.
///
/// Serialises as `{ url, key? }` with a 0x-prefixed key, the form a link carries.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "js", derive(tsify::Tsify))]
pub struct Pointer {
	#[serde(serialize_with = "url_string")]
	#[cfg_attr(feature = "js", tsify(type = "string"))]
	url: Url,
	#[serde(serialize_with = "hex_key", skip_serializing_if = "Option::is_none")]
	#[cfg_attr(feature = "js", tsify(type = "string", optional))]
	key: Option<[u8; 32]>,
}

fn url_string<S: Serializer>(url: &Url, serializer: S) -> Result<S::Ok, S::Error> {
	serializer.serialize_str(url.as_str())
}

fn hex_key<S: Serializer>(key: &Option<[u8; 32]>, serializer: S) -> Result<S::Ok, S::Error> {
	match key {
		Some(key) => serializer.serialize_str(&format::to_hex(key)),
		None => serializer.serialize_none(),
	}
}

impl Pointer {
	/// A pointer from a descriptor URL and an optional 0x-prefixed public key.
	pub fn new(url: &str, key: Option<&str>) -> Result<Self, Error> {
		Ok(Self {
			url: format::url(url, Schemes::Web)?,
			key: key.map(|key| format::hex_bytes::<32>("key", key)).transpose()?,
		})
	}

	/// Reads the pointer from a link's query, whatever scheme or domain the link uses. Parameters
	/// other than `url` and `key` are ignored.
	pub fn from_link(link: &str) -> Result<Self, Error> {
		let parsed = Url::parse(link).map_err(|error| Error::InvalidLink(error.to_string()))?;
		let (mut url, mut key) = (None, None);
		for (name, value) in parsed.query_pairs() {
			let slot = match name.as_ref() {
				"url" => &mut url,
				"key" => &mut key,
				_ => continue,
			};
			if slot.replace(value).is_some() {
				return Err(Error::InvalidLink(format!("{name} appears more than once")));
			}
		}
		let url = url.ok_or_else(|| Error::InvalidLink("no url parameter".into()))?;
		Self::new(&url, key.as_deref())
	}

	/// The link that carries this pointer, under `base` such as [`DEFAULT_LINK_BASE`].
	pub fn to_link(&self, base: &str) -> String {
		let mut query = form_urlencoded::Serializer::new(String::new());
		if let Some(key) = &self.key {
			query.append_pair("key", &format::to_hex(key));
		}
		query.append_pair("url", self.url.as_str());
		let separator = if base.contains('?') { '&' } else { '?' };
		format!("{base}{separator}{}", query.finish())
	}

	/// The descriptor's URL.
	pub fn url(&self) -> &Url {
		&self.url
	}

	/// The public key, when the stack is signed.
	pub fn key(&self) -> Option<&[u8; 32]> {
		self.key.as_ref()
	}

	/// What identifies the stack: its key in hex, or its normalised URL when it has no key. A
	/// signed stack stays the same stack when its URL moves. Readers key what they store by it.
	pub fn stack_id(&self) -> String {
		match &self.key {
			Some(key) => format::to_hex(key),
			None => identity(&self.url),
		}
	}

	/// Whether both pointers name the same stack.
	pub fn same_stack(&self, other: &Self) -> bool {
		self.stack_id() == other.stack_id()
	}
}

/// An unsigned stack's identity: its URL without the fragment, RFC 3986 normalised. The URL
/// parser already normalises case, ports and dot segments; this adds percent-encoding
/// (section 6.2.2), decoding unreserved characters and uppercasing the remaining escapes.
fn identity(url: &Url) -> String {
	let mut url = url.clone();
	url.set_fragment(None);
	let mut normalised = String::with_capacity(url.as_str().len());
	let mut rest = url.as_str();
	while let Some(start) = rest.find('%') {
		normalised.push_str(&rest[..start]);
		let escape = &rest[start..];
		match escape.get(1..3).filter(|hex| hex.bytes().all(|byte| byte.is_ascii_hexdigit())) {
			Some(hex) => {
				let byte = u8::from_str_radix(hex, 16).expect("two hex digits");
				if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
					normalised.push(char::from(byte));
				} else {
					normalised.push('%');
					normalised.push_str(&hex.to_ascii_uppercase());
				}
				rest = &escape[3..];
			},
			None => {
				normalised.push('%');
				rest = &escape[1..];
			},
		}
	}
	normalised.push_str(rest);
	normalised
}

#[cfg(test)]
mod tests;
