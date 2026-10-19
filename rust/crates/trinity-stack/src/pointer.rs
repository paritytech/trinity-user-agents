use crate::{
	Error,
	format::{self, Schemes},
};
use url::{Url, form_urlencoded};

/// The link base the Polkadot app opens.
pub const DEFAULT_LINK_BASE: &str = "polkadotapp://stack";

/// Where a stack's descriptor lives, and the key that signs it when it is signed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pointer {
	url: Url,
	key: Option<[u8; 32]>,
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
mod tests {
	use super::*;

	const KEY: &str = "0x3b6a27bcceb6a42d62a3a8d02a6f0d73653215771de243a63ac048a18b59da29";
	const URL: &str = "https://previewnet.substrate.dev/.well-known/trinity-stack.json";

	#[test]
	fn the_rfc_link_round_trips() {
		let link = format!(
			"polkadotapp://stack?key={KEY}&url=https%3A%2F%2Fpreviewnet.substrate.dev%2F.well-known%2Ftrinity-stack.json"
		);
		let pointer = Pointer::from_link(&link).unwrap();
		assert_eq!(pointer, Pointer::new(URL, Some(KEY)).unwrap());
		assert_eq!(pointer.to_link(DEFAULT_LINK_BASE), link);
	}

	#[test]
	fn any_scheme_carries_a_pointer_and_extra_parameters_are_ignored() {
		let link = format!("https://app.example/open?utm=x&url={URL}");
		assert_eq!(Pointer::from_link(&link), Pointer::new(URL, None));
	}

	#[test]
	fn a_repeated_parameter_is_refused() {
		let link = format!("polkadotapp://stack?url={URL}&url=https://other.example/");
		assert_eq!(Pointer::from_link(&link).map_err(|error| error.code()), Err("invalid-link"));
	}

	#[test]
	fn same_stack_follows_the_key() {
		let signed = Pointer::new(URL, Some(KEY)).unwrap();
		let moved = Pointer::new("https://elsewhere.example/stack.json", Some(KEY)).unwrap();
		let unsigned = Pointer::new(URL, None).unwrap();
		assert!(signed.same_stack(&moved));
		assert!(!signed.same_stack(&unsigned));
		assert!(unsigned.same_stack(&Pointer::new(URL, None).unwrap()));
		assert_eq!(signed.stack_id(), KEY);
		assert_eq!(
			unsigned.stack_id(),
			Pointer::new(&format!("{URL}#x"), None).unwrap().stack_id()
		);
	}

	#[test]
	fn an_unsigned_stack_is_its_url_normalised_without_the_fragment() {
		let unsigned = Pointer::new("https://a.example/~s%2fx.json", None).unwrap();
		for same in ["https://a.example/%7Es%2Fx.json#top", "HTTPS://A.EXAMPLE:443/~s%2Fx.json"] {
			assert!(unsigned.same_stack(&Pointer::new(same, None).unwrap()), "{same}");
		}
		let other = Pointer::new("https://a.example/~s/x.json", None).unwrap();
		assert!(!unsigned.same_stack(&other), "%2F is not a path separator");
	}

	#[test]
	fn a_link_base_may_carry_its_own_query() {
		let pointer = Pointer::new(URL, Some(KEY)).unwrap();
		let link = pointer.to_link("https://app.example/open?ref=qr");
		assert_eq!(Pointer::from_link(&link), Ok(pointer));
	}
}
