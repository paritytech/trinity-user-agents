//! dotns URL parsing, normalization, and classification.
//!
//! The Rust core owns the whole decision so every platform host sees the
//! same categorization and the `navigate_to` callback only receives
//! already-validated input.

use crate::{
	dotns_name::normalize_host,
	platform::{ProductId, normalize_chat_identifier},
	stack::{served_tlds, serves, serves_tld},
};
use url::{Url, form_urlencoded};

/// The app's own URL scheme, which its host routes such as `pair` and
/// `pocket` live under.
pub const APP_SCHEME: &str = "polkadotapp";

/// The internal URL scheme under which hosts serve product content.
pub const PRODUCT_SCHEME: &str = "polkadot";

/// Public share hosts append this suffix to a complete dotNS identity.
pub const PRODUCT_SHARE_SUFFIX: &str = ".li";

const HTTP_SCHEME: &str = "http";
const HTTPS_SCHEME: &str = "https";

/// The host a Pocket deeplink names. Every dotNS product name ends in a TLD,
/// so no product can take it.
const POCKET_HOST: &str = "pocket";

/// What a `polkadotapp://pocket/<action>` deeplink asks the host to do.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum PocketDeeplinkAction {
	/// Offer to add a published card, behind the host's approval dialog.
	Add,
	/// Expand a card that is already present.
	Open,
}

/// How the input URL should be opened. Kept in one enum rather than passing
/// a raw string so the dispatcher can reject invalid input before reaching
/// any platform callback. The open variants carry the ready-to-load canonical
/// URL; `DotName` and `Localhost` keep the dotns/localhost identity visible so
/// env-aware hosts can rewrite dotNS names for their active environment and
/// re-parse without losing information.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum NavigateDecision {
	/// A dotNS identifier plus path/query/hash suffix (no leading `/`).
	DotName {
		/// Lower-cased dotNS host (e.g. `mytestapp.dot`).
		identifier: ProductId,
		/// Path/query/hash suffix without a leading `/`.
		path: String,
		/// Normalized `polkadot://` form, which is what `navigate_to` hands the
		/// host. Hosts route on the scheme, and no dotNS TLD resolves in public
		/// DNS, so an `https://` string would not load either.
		canonical_url: String,
	},
	/// A `localhost[:port]` URL plus path/query/hash suffix (no leading `/`).
	Localhost {
		/// `localhost` with optional `:port` suffix.
		host: ProductId,
		/// Path/query/hash suffix without a leading `/`.
		path: String,
		/// Loadable `http://` URL for this decision.
		canonical_url: String,
	},
	/// A Pocket action on one card of a dotNS product:
	/// `polkadotapp://pocket/<action>?product=<id>&card=<id>`. The deeplink is the
	/// host's own, so no path of any product is read as one.
	Pocket {
		/// Normalized dotNS name of the product that backs the card, from the
		/// `product` query parameter.
		identifier: ProductId,
		/// Requested Pocket action.
		action: PocketDeeplinkAction,
		/// Card named by the `card` query parameter.
		card_id: String,
		/// An `open` link's other query parameters, still encoded, for the
		/// card's page. Anyone can write the link, so they are a hint, never proof.
		page_query: String,
		/// Normalized `polkadotapp://` form of the deeplink, which is what
		/// `navigate_to` hands the host. The product and card are
		/// percent-encoded, so re-parsing this names the same card.
		canonical_url: String,
	},
	/// The Pocket itself: `polkadotapp://pocket` with no action. It names no card,
	/// so the host opens the collection rather than a card.
	PocketCollection {
		/// Normalized `polkadotapp://` form of the deeplink, which is what
		/// `navigate_to` hands the host.
		canonical_url: String,
	},
	/// An absolute external URL with an `http(s):` scheme prepended if missing.
	External {
		/// Canonical URL string.
		url: String,
	},
	/// Input that fails every branch: empty, unparseable, or a dotNS URL
	/// carrying port/userinfo (both forbidden since dotns resolves via the
	/// chain and has no notion of either).
	Reject {
		/// Human-readable reason for the rejection.
		reason: String,
	},
}

/// A product destination verified against a host's explicit network suffix.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct ProductLink {
	/// The full executable or product identity, with a share suffix removed.
	pub identifier: ProductId,
	/// The route with its leading slash, query and fragment.
	pub path: String,
	/// The HTTPS destination under the canonical dotNS host.
	pub canonical_url: String,
}

impl ProductLink {
	/// Accept direct and share URLs only on the explicitly configured network.
	pub fn parse(input: &str, network_suffix: &str) -> Option<Self> {
		let parsed = parse_with_explicit_https(input.trim())?;
		if !matches!(parsed.scheme(), HTTP_SCHEME | HTTPS_SCHEME | APP_SCHEME | PRODUCT_SCHEME) ||
			parsed.port().is_some() ||
			!parsed.username().is_empty() ||
			parsed.password().is_some()
		{
			return None;
		}
		let hostname = parsed.host_str()?;
		// The direct reading is tried too: on a network whose TLD is the share
		// suffix, the share reading names a product one label shorter.
		let identifier = ProductId::from_share_host(hostname, network_suffix)
			.into_iter()
			.chain(normalize_host(hostname).parse::<ProductId>())
			.find(|identifier| identifier.matches_network(network_suffix))?;
		let route = if parsed.path().is_empty() { "/" } else { parsed.path() };
		let path = route.to_string() + &suffix(&parsed);
		let canonical_url = format!("{HTTPS_SCHEME}://{identifier}{path}");
		Some(Self { identifier, path, canonical_url })
	}
}

/// Public share domains of the TLDs a stack on `network_suffix` serves.
pub fn product_share_hosts(network_suffix: &str) -> Vec<String> {
	served_tlds(network_suffix)
		.map(|tld| format!("{tld}{PRODUCT_SHARE_SUFFIX}"))
		.collect()
}

fn join_url(scheme: &str, host: &str, path: &str) -> String {
	if path.is_empty() { format!("{scheme}://{host}") } else { format!("{scheme}://{host}/{path}") }
}

/// Classify a URL the way the host navigation handler does: try the Pocket
/// deeplink first, then dotNS, then `localhost`, then normalize as external.
///
/// `network_suffix` is the TLD of the host's stack. A name is dotNS only under
/// a TLD that stack serves, so any other host is external.
pub fn parse_navigate(input: &str, network_suffix: &str) -> NavigateDecision {
	let trimmed = input.trim();
	if trimmed.is_empty() {
		return NavigateDecision::Reject { reason: "empty input".to_string() };
	}

	if let Some(decision) = classify_pocket(trimmed, network_suffix) {
		return decision;
	}

	if let Some(decision) = classify_dotns(trimmed, network_suffix) {
		return decision;
	}

	if let Some(decision) = classify_localhost(trimmed) {
		return decision;
	}

	match normalize_external(trimmed) {
		Ok(url) => NavigateDecision::External { url },
		Err(reason) => NavigateDecision::Reject { reason },
	}
}

/// Whether `host` ends in a TLD a stack on `network_suffix` serves, applied to
/// the [`normalize_host`] form so `Example.DOT` and the trailing-dot FQDN
/// `example.dot.` classify like `example.dot`.
fn is_dotns_domain(host: &str, network_suffix: &str) -> bool {
	normalize_host(host)
		.rsplit_once('.')
		.is_some_and(|(_, tld)| serves_tld(network_suffix, tld))
}

fn parse_with_explicit_https(input: &str) -> Option<Url> {
	if let Ok(direct) = Url::parse(input) {
		return Some(direct);
	}
	Url::parse(&format!("{HTTPS_SCHEME}://{input}")).ok()
}

/// Recognize the host's own Pocket deeplink, `polkadotapp://pocket` with an
/// optional action. Only the app's scheme names it: under a web scheme `pocket`
/// is an ordinary host name, so a host whose build answers to another flavor
/// of the scheme maps it to [`APP_SCHEME`] before asking. Recognized ahead of
/// the other `polkadotapp:` links, which need an `OpenUrl` grant: the Pocket is
/// the host's own surface. `None` sends the input on to the other branches.
fn classify_pocket(input: &str, network_suffix: &str) -> Option<NavigateDecision> {
	let url = Url::parse(input).ok().filter(|url| url.scheme() == APP_SCHEME)?;
	if normalize_host(url.host_str()?) != POCKET_HOST {
		return None;
	}
	let target: Vec<&str> = url
		.path_segments()
		.into_iter()
		.flatten()
		.filter(|segment| !segment.is_empty())
		.collect();
	let (action, verb) = match target.as_slice() {
		[] => {
			return Some(NavigateDecision::PocketCollection {
				canonical_url: format!("{APP_SCHEME}://{POCKET_HOST}"),
			});
		},
		["add"] => (PocketDeeplinkAction::Add, "add"),
		["open"] => (PocketDeeplinkAction::Open, "open"),
		// Every `polkadotapp://pocket` link is the host's, so an action this core
		// does not serve is refused rather than opened as something else.
		_ => {
			return Some(NavigateDecision::Reject {
				reason: "pocket deeplink names an action this core does not serve".to_string(),
			});
		},
	};
	let argument = |name: &str| {
		url.query_pairs()
			.find(|(key, _)| key == name)
			.map(|(_, value)| value.into_owned())
	};
	let (Some(product), Some(card_id)) = (argument("product"), argument("card")) else {
		return Some(NavigateDecision::Reject {
			reason: "pocket deeplink names no product or no card".to_string(),
		});
	};
	let identifier = match product.parse::<ProductId>() {
		Ok(identifier) if !identifier.is_localhost() && serves(network_suffix, &identifier) => {
			identifier
		},
		_ => {
			return Some(NavigateDecision::Reject {
				reason: "pocket deeplink names a product that is not a dotNS name".to_string(),
			});
		},
	};
	// A deeplink and `remove_card` name the same card, so both screen the id
	// the same way. Without this, `?card=%20loyalty%20` would add a card under
	// a name removal can never match.
	let card_id = match normalize_chat_identifier("card", &card_id) {
		Ok(card_id) => card_id,
		Err(error) => {
			return Some(NavigateDecision::Reject { reason: error.to_string() });
		},
	};
	// Left encoded, so the page reads its params exactly as the link wrote them.
	let page_query = match action {
		PocketDeeplinkAction::Open => url
			.query()
			.unwrap_or_default()
			.split('&')
			.filter(|pair| {
				!pair.is_empty() &&
					!form_urlencoded::parse(pair.as_bytes())
						.any(|(key, _)| key == "product" || key == "card")
			})
			.collect::<Vec<_>>()
			.join("&"),
		PocketDeeplinkAction::Add => String::new(),
	};
	// `url` leaves bytes such as `{` or a lone `%` raw under a custom scheme,
	// and the hosts' https URL parsers refuse them.
	let legal_bytes = page_query
		.bytes()
		.all(|byte| byte.is_ascii_alphanumeric() || b"-._~!$&'()*+,;=:@/?%".contains(&byte));
	let whole_escapes = page_query.split('%').skip(1).all(|escape| {
		escape
			.get(..2)
			.is_some_and(|hex| hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
	});
	if page_query.len() > 2048 || !legal_bytes || !whole_escapes {
		return Some(NavigateDecision::Reject {
			reason: "pocket deeplink page params are too long or not URL-encoded".into(),
		});
	}
	// The arguments arrive percent-decoded, so they are encoded again here: a
	// raw `#` or `&` in a card id would make the canonical form parse as a
	// different deeplink.
	let query = form_urlencoded::Serializer::new(String::new())
		.append_pair("product", identifier.as_str())
		.append_pair("card", &card_id)
		.finish();
	let separator = if page_query.is_empty() { "" } else { "&" };
	Some(NavigateDecision::Pocket {
		canonical_url: format!(
			"{APP_SCHEME}://{POCKET_HOST}/{verb}?{query}{separator}{page_query}"
		),
		identifier,
		action,
		card_id,
		page_query,
	})
}

/// Recognize dotNS URLs (including the `polkadot://` scheme). Returns:
/// - `Some(DotName)` for a clean dotNS URL
/// - `Some(Reject)` for a dotNS URL with port or userinfo
/// - `None` when the input isn't a dotNS URL (caller falls through to localhost / external)
fn classify_dotns(input: &str, network_suffix: &str) -> Option<NavigateDecision> {
	let parsed = parse_with_explicit_https(input)?;

	let hostname = parsed.host_str()?;
	if !is_dotns_domain(hostname, network_suffix) {
		return None;
	}

	if parsed.port().is_some() || !parsed.username().is_empty() || parsed.password().is_some() {
		return Some(NavigateDecision::Reject {
			reason: format!("{hostname} carries port or userinfo; dotns forbids both"),
		});
	}

	let identifier = match normalize_host(hostname).parse::<ProductId>() {
		Ok(identifier) => identifier,
		Err(error) => return Some(NavigateDecision::Reject { reason: error.to_string() }),
	};
	let path = strip_leading_slash(parsed.path()) + &suffix(&parsed);
	let canonical_url = join_url(PRODUCT_SCHEME, identifier.as_str(), &path);
	Some(NavigateDecision::DotName { identifier, path, canonical_url })
}

/// Recognize `localhost[:port]` URLs, with or without an explicit scheme.
fn classify_localhost(input: &str) -> Option<NavigateDecision> {
	let with_scheme = if input.starts_with("localhost") {
		format!("{HTTP_SCHEME}://{input}")
	} else {
		input.to_string()
	};

	let parsed = Url::parse(&with_scheme).ok()?;
	if parsed.host_str()? != "localhost" {
		return None;
	}

	let host = match parsed.port() {
		Some(port) => format!("localhost:{port}"),
		None => "localhost".to_string(),
	};

	let host = host
		.parse::<ProductId>()
		.expect("localhost with an optional u16 port is a valid product id; qed");
	let path = strip_leading_slash(parsed.path()) + &suffix(&parsed);
	let canonical_url = join_url(HTTP_SCHEME, host.as_str(), &path);
	Some(NavigateDecision::Localhost { host, path, canonical_url })
}

/// External URL scheme allowlist. Anything outside this set is treated as
/// a [`NavigateDecision::Reject`] so dangerous schemes (`javascript:`,
/// `data:`, `file:`, `vbscript:`, ...) cannot reach `Platform::navigate_to`.
const ALLOWED_EXTERNAL_SCHEMES: &[&str] =
	&[HTTP_SCHEME, HTTPS_SCHEME, "mailto", "tel", "sms", "maps", APP_SCHEME];

/// Mirrors `normalizeUrl`: prepend `https://` if missing, otherwise pass the
/// URL through as its canonical string form. Returns `Err(reason)` for an
/// unparseable input or a scheme outside [`ALLOWED_EXTERNAL_SCHEMES`].
fn normalize_external(input: &str) -> Result<String, String> {
	// `parse_with_explicit_https` returns a successful direct parse as-is and
	// only prepends `https://` when the direct parse fails, so a disallowed
	// scheme (e.g. `javascript:`) is never rewritten to https: the single
	// scheme check below rejects it.
	let url = parse_with_explicit_https(input)
		.ok_or_else(|| "URL constructor rejected input".to_string())?;
	if !ALLOWED_EXTERNAL_SCHEMES.contains(&url.scheme()) {
		return Err(format!("scheme `{}` is not allowed", url.scheme()));
	}
	Ok(url.to_string())
}

fn strip_leading_slash(path: &str) -> String {
	path.strip_prefix('/').unwrap_or(path).to_string()
}

fn suffix(url: &Url) -> String {
	let mut out = String::new();
	if let Some(q) = url.query() {
		out.push('?');
		out.push_str(q);
	}
	if let Some(f) = url.fragment() {
		out.push('#');
		out.push_str(f);
	}
	out
}

#[cfg(test)]
mod tests;
