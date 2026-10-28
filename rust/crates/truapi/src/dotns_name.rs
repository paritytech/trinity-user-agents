//! The names dotNS can register: its top-level domains and its label syntax.

#[cfg(feature = "runtime")]
use unicode_normalization::UnicodeNormalization;

/// Top-level domains dotNS registers names under, one per network a host can
/// use, not only production.
pub const DOTNS_TLDS: &[&str] = &["dot", "paseo", "testnet"];

/// Whether `normalized` ends in one of [`DOTNS_TLDS`]. Expects an
/// already-lowercased host with no trailing root dot.
pub fn has_dotns_tld(normalized: &str) -> bool {
	normalized.rsplit_once('.').is_some_and(|(_, tld)| DOTNS_TLDS.contains(&tld))
}

/// RFC 1035 label bound the contracts enforce (`StringUtils.MAX_DNS_LABEL_OCTETS`).
pub const MAX_DNS_LABEL_LEN: usize = 63;

/// A canonical DNS label: lowercase ASCII letters, digits and hyphens, neither
/// starting nor ending with a hyphen, at most 63 octets
/// (`StringUtils._isDnsLabel`).
pub fn is_dns_label(value: &str) -> bool {
	!value.is_empty() &&
		value.len() <= MAX_DNS_LABEL_LEN &&
		!value.starts_with('-') &&
		!value.ends_with('-') &&
		value.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Canonical host form: case-folded and NFC-normalized (belt-and-suspenders;
/// `url` already applies IDNA to parsed hosts), with a trailing root dot
/// dropped so the absolute form `example.dot.` keys identically to
/// `example.dot`.
#[cfg(feature = "runtime")]
pub fn normalize_host(host: &str) -> String {
	let normalized: String = host.nfc().collect::<String>().to_lowercase();
	normalized.strip_suffix('.').unwrap_or(&normalized).to_string()
}
