//! The value formats the RFC fixes: hex and URLs.

use crate::Error;
use std::net::{Ipv4Addr, Ipv6Addr};
use url::{Host, Url};

/// Decodes lowercase 0x-prefixed hex of exactly `N` bytes.
pub fn hex_bytes<const N: usize>(field: &'static str, value: &str) -> Result<[u8; N], Error> {
	let invalid = || Error::InvalidHex { field, bytes: N };
	let digits = value.strip_prefix("0x").ok_or_else(invalid)?;
	if digits.bytes().any(|byte| byte.is_ascii_uppercase()) {
		return Err(invalid());
	}
	let mut bytes = [0; N];
	hex::decode_to_slice(digits, &mut bytes).map_err(|_| invalid())?;
	Ok(bytes)
}

/// Encodes bytes as lowercase 0x-prefixed hex.
pub fn to_hex(bytes: &[u8]) -> String {
	format!("0x{}", hex::encode(bytes))
}

/// Serialises bytes as bytes, not a sequence of numbers: a `Uint8Array` in JS.
pub fn serialize_bytes<S: serde::Serializer>(
	bytes: &[u8],
	serializer: S,
) -> Result<S::Ok, S::Error> {
	serializer.serialize_bytes(bytes)
}

/// Which schemes a URL may use where it appears.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Schemes {
	/// `https` or `wss`, or `http` or `ws` on a private-range literal address.
	Web,
	/// `stun`, for the URLs of a `stun` service.
	Stun,
}

/// Parses a URL and checks it against the RFC's scheme rule. Descriptors keep the text as written,
/// so the text itself must be the URL: the parser would forgive whitespace, control characters
/// and a missing `//`, which other readers refuse.
pub fn url(value: &str, schemes: Schemes) -> Result<Url, Error> {
	let invalid = || Error::InvalidUrl(value.to_owned());
	if value
		.chars()
		.any(|character| character.is_whitespace() || character.is_control())
	{
		return Err(invalid());
	}
	let parsed = Url::parse(value).map_err(|_| invalid())?;
	let hierarchical = value
		.split_once("://")
		.is_some_and(|(scheme, _)| scheme.eq_ignore_ascii_case(parsed.scheme()));
	// A user name or password would let `https://trusted.example@evil.example/` read as trusted.
	let no_credentials = parsed.username().is_empty() && parsed.password().is_none();
	let web = hierarchical && no_credentials && canonical_address(&parsed, value);
	let allowed = match (schemes, parsed.scheme()) {
		(Schemes::Stun, scheme) => scheme == "stun",
		(Schemes::Web, "https" | "wss") => web,
		(Schemes::Web, "http" | "ws") => web && parsed.host().is_some_and(is_private_literal),
		(Schemes::Web, _) => false,
	};
	if allowed { Ok(parsed) } else { Err(invalid()) }
}

/// An address host must be written in canonical form. The URL parser reads shorthand such as
/// `10.1` or `0x7f.1` as an IPv4 address, which RFC 3986 parsers read as a hostname, so two
/// readers would reach different hosts.
fn canonical_address(parsed: &Url, value: &str) -> bool {
	match (parsed.host(), parsed.host_str()) {
		(Some(Host::Domain(_)), _) => true,
		(Some(_), Some(host)) => host.eq_ignore_ascii_case(written_host(value)),
		_ => false,
	}
}

/// The host exactly as written in a hierarchical URL.
fn written_host(value: &str) -> &str {
	let after_scheme = value.split_once("://").map_or("", |(_, rest)| rest);
	let authority = after_scheme.split(['/', '?', '#']).next().unwrap_or_default();
	let host_and_port = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
	match host_and_port.find(']') {
		Some(end) if host_and_port.starts_with('[') => &host_and_port[..=end],
		_ => host_and_port.split(':').next().unwrap_or_default(),
	}
}

/// A literal address in one of the private ranges the RFC lists. A hostname never qualifies.
fn is_private_literal(host: Host<&str>) -> bool {
	match host {
		Host::Ipv4(address) => is_private_v4(address),
		Host::Ipv6(address) => is_private_v6(address),
		Host::Domain(_) => false,
	}
}

fn is_private_v4(address: Ipv4Addr) -> bool {
	address.is_loopback() || address.is_private()
}

fn is_private_v6(address: Ipv6Addr) -> bool {
	let first = address.segments()[0];
	address.is_loopback() || first & 0xfe00 == 0xfc00 || first & 0xffc0 == 0xfe80
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn plain_http_only_on_private_literal_addresses() {
		for allowed in [
			"https://a.example/",
			"wss://a.example",
			"http://10.0.0.5/",
			"ws://[::1]:9944",
			"http://192.168.1.2",
			"wss://8.8.8.8/",
			"https://A.Example/",
		] {
			assert!(url(allowed, Schemes::Web).is_ok(), "{allowed}");
		}
		for refused in [
			"http://localhost/",
			"http://8.8.8.8/",
			"ftp://a.example/",
			"stun:a.example:3478",
			"http://[2001:db8::1]/",
			"ws://10.1:9944",
			"ws://0x7f.1/",
			"http://user@10.1/",
			"https://previewnet.substrate.dev@evil.example/",
			"wss://user:secret@a.example/",
			"wss://10.1/",
			"https://0x08080808/",
		] {
			assert!(url(refused, Schemes::Web).is_err(), "{refused}");
		}
		assert!(url("stun:stun.example:3478", Schemes::Stun).is_ok());
		assert!(url("https://a.example/", Schemes::Stun).is_err());
	}

	#[test]
	fn a_url_is_read_as_written() {
		for refused in
			[" https://a.example/", "https://a.example/\n", "https://a.exa\tmple/", "wss:a.example"]
		{
			assert!(url(refused, Schemes::Web).is_err(), "{refused:?}");
		}
		assert!(url("stun:stun.example:3478 ", Schemes::Stun).is_err());
	}

	#[test]
	fn hex_is_lowercase_and_sized() {
		assert_eq!(hex_bytes::<2>("f", "0xabcd"), Ok([0xab, 0xcd]));
		for invalid in ["abcd", "0xABCD", "0xabc", "0xabcdef"] {
			assert_eq!(
				hex_bytes::<2>("f", invalid),
				Err(Error::InvalidHex { field: "f", bytes: 2 })
			);
		}
	}
}
