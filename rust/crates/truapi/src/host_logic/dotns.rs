//! dotns URL parsing, normalization, and classification.
//!
//! The Rust core owns the whole decision so every platform host sees the
//! same categorization and the `navigate_to` callback only receives
//! already-validated input.

use crate::{
	dotns_name::normalize_host,
	platform::{ProductId, has_dotns_tld, normalize_chat_identifier},
};
use url::{Url, form_urlencoded};

/// The app's own URL scheme, which its host routes such as `pair` and
/// `pocket` live under.
const APP_SCHEME: &str = "polkadotapp";

/// The host a Pocket deeplink names. Every dotNS product name ends in a TLD,
/// so no product can take it.
const POCKET_HOST: &str = "pocket";

/// What a `polkadotapp://pocket/<action>` deeplink asks the host to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
		if !matches!(parsed.scheme(), "http" | "https" | "polkadot") ||
			parsed.port().is_some() ||
			!parsed.username().is_empty() ||
			parsed.password().is_some()
		{
			return None;
		}
		let hostname = normalize_host(parsed.host_str()?);
		let identifier =
			ProductId::from_share_host(&hostname).or_else(|| hostname.parse::<ProductId>().ok())?;
		if !identifier.matches_network(network_suffix) {
			return None;
		}
		let route = if parsed.path().is_empty() { "/" } else { parsed.path() };
		let path = route.to_string() + &suffix(&parsed);
		let canonical_url = format!("https://{identifier}{path}");
		Some(Self { identifier, path, canonical_url })
	}
}

/// Public share domains for the canonical dotNS networks.
pub fn product_share_hosts() -> Vec<String> {
	crate::dotns_name::DOTNS_TLDS
		.iter()
		.map(|suffix| format!("{suffix}.li"))
		.collect()
}

fn join_url(scheme: &str, host: &str, path: &str) -> String {
	if path.is_empty() { format!("{scheme}{host}") } else { format!("{scheme}{host}/{path}") }
}

/// Classify a URL the way the host navigation handler does: try the Pocket
/// deeplink first, then dotNS, then `localhost`, then normalize as external.
pub fn parse_navigate(input: &str) -> NavigateDecision {
	let trimmed = input.trim();
	if trimmed.is_empty() {
		return NavigateDecision::Reject { reason: "empty input".to_string() };
	}

	if let Some(decision) = classify_pocket(trimmed) {
		return decision;
	}

	if let Some(decision) = classify_dotns(trimmed) {
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

/// dotNS TLD check, applied to the [`normalize_host`] form so `Example.DOT`
/// and the trailing-dot FQDN `example.dot.` classify like `example.dot`.
/// Shares [`crate::platform::DOTNS_TLDS`] with product-identifier validation
/// so navigation and derivation accept the same per-network names.
fn is_dotns_domain(host: &str) -> bool {
	has_dotns_tld(&normalize_host(host))
}

fn parse_with_explicit_https(input: &str) -> Option<Url> {
	if let Ok(direct) = Url::parse(input) {
		return Some(direct);
	}
	Url::parse(&format!("https://{input}")).ok()
}

/// Recognize the host's own Pocket deeplink, `polkadotapp://pocket` with an
/// optional action. Only the app's scheme names it: under a web scheme `pocket`
/// is an ordinary host name, so a host whose build answers to another flavor
/// of the scheme maps it to [`APP_SCHEME`] before asking. Recognized ahead of
/// the other `polkadotapp:` links, which need an `OpenUrl` grant: the Pocket is
/// the host's own surface. `None` sends the input on to the other branches.
fn classify_pocket(input: &str) -> Option<NavigateDecision> {
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
		Ok(identifier) if has_dotns_tld(identifier.as_str()) => identifier,
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
	// The arguments arrive percent-decoded, so they are encoded again here: a
	// raw `#` or `&` in a card id would make the canonical form parse as a
	// different deeplink.
	let query = form_urlencoded::Serializer::new(String::new())
		.append_pair("product", identifier.as_str())
		.append_pair("card", &card_id)
		.finish();
	Some(NavigateDecision::Pocket {
		canonical_url: format!("{APP_SCHEME}://{POCKET_HOST}/{verb}?{query}"),
		identifier,
		action,
		card_id,
	})
}

/// Recognize dotNS URLs (including the `polkadot://` scheme). Returns:
/// - `Some(DotName)` for a clean dotNS URL
/// - `Some(Reject)` for a dotNS URL with port or userinfo
/// - `None` when the input isn't a dotNS URL (caller falls through to localhost / external)
fn classify_dotns(input: &str) -> Option<NavigateDecision> {
	let parsed = if input.starts_with("polkadot://") {
		Url::parse(input).ok()?
	} else {
		parse_with_explicit_https(input)?
	};

	let hostname = parsed.host_str()?;
	if !is_dotns_domain(hostname) {
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
	let canonical_url = join_url("polkadot://", identifier.as_str(), &path);
	Some(NavigateDecision::DotName { identifier, path, canonical_url })
}

/// Recognize `localhost[:port]` URLs, with or without an explicit scheme.
fn classify_localhost(input: &str) -> Option<NavigateDecision> {
	let with_scheme =
		if input.starts_with("localhost") { format!("http://{input}") } else { input.to_string() };

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
	let canonical_url = join_url("http://", host.as_str(), &path);
	Some(NavigateDecision::Localhost { host, path, canonical_url })
}

/// External URL scheme allowlist. Anything outside this set is treated as
/// a [`NavigateDecision::Reject`] so dangerous schemes (`javascript:`,
/// `data:`, `file:`, `vbscript:`, ...) cannot reach `Platform::navigate_to`.
const ALLOWED_EXTERNAL_SCHEMES: &[&str] =
	&["http", "https", "mailto", "tel", "sms", "maps", "polkadotapp"];

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
mod tests {
	use super::*;

	#[test]
	fn product_links_require_the_explicit_network_for_direct_and_share_forms() {
		for suffix in crate::dotns_name::DOTNS_TLDS {
			for mirror in ["", ".li"] {
				let input = format!("https://app.arena.coinflip.{suffix}{mirror}/play?mode=1#turn");
				assert_eq!(
					ProductLink::parse(&input, suffix),
					Some(ProductLink {
						identifier: ProductId::for_test(&format!("app.arena.coinflip.{suffix}")),
						path: "/play?mode=1#turn".to_string(),
						canonical_url: format!(
							"https://app.arena.coinflip.{suffix}/play?mode=1#turn"
						),
					})
				);
				for other in crate::dotns_name::DOTNS_TLDS.iter().filter(|other| *other != suffix) {
					assert_eq!(ProductLink::parse(&input, other), None);
				}
			}
		}
	}

	#[test]
	fn product_links_normalize_supported_url_forms_and_preserve_routes() {
		for (input, path) in [
			("browse.dot", "/"),
			("https://browse.dot/", "/"),
			("polkadot://browse.dot", "/"),
			("browse.dot.li/#/onboarding", "/#/onboarding"),
			("browse.dot/onboarding?ref=abc", "/onboarding?ref=abc"),
			("http://browse.dot.li?q=1#fragment", "/?q=1#fragment"),
			(" HTTP://BROWSE.DOT.LI./a%20b?q=x%2Fy#frag ", "/a%20b?q=x%2Fy#frag"),
		] {
			assert_eq!(
				ProductLink::parse(input, "dot"),
				Some(ProductLink {
					identifier: ProductId::for_test("browse.dot"),
					path: path.to_string(),
					canonical_url: format!("https://browse.dot{path}"),
				}),
				"{input}"
			);
		}
	}

	#[test]
	fn product_links_reject_non_product_authorities() {
		for input in [
			"",
			"browse.test.li",
			"browse.com",
			"localhost:3000",
			"localhost.dot.li",
			"https://browse.dot:8443/path",
			"https://user@browse.dot.li/path",
			"javascript://browse.dot",
			"https://invalid_name.dot.li",
			"https://dot.li",
			"https://stg.revx.dev/editor?mod=other.dot",
		] {
			assert_eq!(ProductLink::parse(input, "dot"), None, "{input}");
		}
	}

	#[test]
	fn share_hosts_use_the_canonical_network_suffixes() {
		assert_eq!(product_share_hosts(), ["dot.li", "paseo.li", "testnet.li"]);
	}

	enum Expected {
		Decision(NavigateDecision),
		Reject,
	}

	struct TestCase {
		name: &'static str,
		input: &'static str,
		expected: Expected,
	}

	fn dot(identifier: &str, path: &str) -> Expected {
		Expected::Decision(NavigateDecision::DotName {
			identifier: ProductId::for_test(identifier),
			path: path.to_string(),
			canonical_url: join_url("polkadot://", identifier, path),
		})
	}

	/// Only for card ids that need no percent-encoding;
	/// `pocket_canonical_url_reencodes_the_card_id` covers the ones that do.
	fn pocket(identifier: &str, action: PocketDeeplinkAction, card_id: &str) -> Expected {
		let verb = match action {
			PocketDeeplinkAction::Add => "add",
			PocketDeeplinkAction::Open => "open",
		};
		Expected::Decision(NavigateDecision::Pocket {
			identifier: ProductId::for_test(identifier),
			action,
			card_id: card_id.to_string(),
			canonical_url: format!(
				"polkadotapp://pocket/{verb}?product={identifier}&card={card_id}"
			),
		})
	}

	fn pocket_collection() -> Expected {
		Expected::Decision(NavigateDecision::PocketCollection {
			canonical_url: "polkadotapp://pocket".to_string(),
		})
	}

	fn localhost(host: &str, path: &str) -> Expected {
		Expected::Decision(NavigateDecision::Localhost {
			host: ProductId::for_test(host),
			path: path.to_string(),
			canonical_url: join_url("http://", host, path),
		})
	}

	fn external(url: &str) -> Expected {
		Expected::Decision(NavigateDecision::External { url: url.to_string() })
	}

	/// Pinned as literals so the expectation does not borrow the production
	/// builder it checks.
	#[test]
	fn product_destinations_require_canonical_product_identifiers() {
		for input in ["polkadot://invalid_name.dot", "localhost.dot", "-invalid.dot"] {
			assert!(matches!(parse_navigate(input), NavigateDecision::Reject { .. }), "{input}");
		}
	}

	#[test]
	fn a_product_destination_is_handed_over_as_a_polkadot_url() {
		for (input, expected) in [
			("calculator.paseo", "polkadot://calculator.paseo"),
			("polkadot://calculator.paseo", "polkadot://calculator.paseo"),
			("https://calculator.paseo", "polkadot://calculator.paseo"),
			(
				"calculator.paseo/deep/path?q=1#frag",
				"polkadot://calculator.paseo/deep/path?q=1#frag",
			),
			("MyTestApp.DOT", "polkadot://mytestapp.dot"),
		] {
			let NavigateDecision::DotName { canonical_url, .. } = parse_navigate(input) else {
				panic!("{input} should classify as a dotNS product destination");
			};
			assert_eq!(canonical_url, expected, "for {input}");
		}
	}

	/// A web address keeps its scheme, so the two stay distinguishable.
	#[test]
	fn a_web_address_is_still_handed_over_as_https() {
		let NavigateDecision::External { url } = parse_navigate("https://example.com/x") else {
			panic!("example.com should classify as external");
		};
		assert_eq!(url, "https://example.com/x");
	}

	#[test]
	fn parse_navigate_cases() {
		let cases = vec![
			TestCase {
				name: "dot bare",
				input: "mytestapp.dot",
				expected: dot("mytestapp.dot", ""),
			},
			TestCase {
				name: "dot trailing root dot",
				input: "example.dot.",
				expected: dot("example.dot", ""),
			},
			TestCase {
				name: "dot trailing root dot with path",
				input: "https://example.dot./path",
				expected: dot("example.dot", "path"),
			},
			TestCase {
				name: "dot li is external",
				input: "mytestapp.dot.li",
				expected: external("https://mytestapp.dot.li/"),
			},
			TestCase {
				name: "dot with https",
				input: "https://mytestapp.dot",
				expected: dot("mytestapp.dot", ""),
			},
			TestCase {
				name: "dot with http",
				input: "http://mytestapp.dot",
				expected: dot("mytestapp.dot", ""),
			},
			TestCase {
				name: "dot with path",
				input: "mytestapp.dot/some/path",
				expected: dot("mytestapp.dot", "some/path"),
			},
			TestCase {
				name: "dot with query only",
				input: "pr508.faucet.dot?embed=1",
				expected: dot("pr508.faucet.dot", "?embed=1"),
			},
			TestCase {
				name: "dot with hash only",
				input: "pr508.faucet.dot#section=main",
				expected: dot("pr508.faucet.dot", "#section=main"),
			},
			TestCase {
				name: "dot with path query hash",
				input: "pr508.faucet.dot/nested/path?embed=1#frame=compact",
				expected: dot("pr508.faucet.dot", "nested/path?embed=1#frame=compact"),
			},
			TestCase {
				name: "polkadot scheme dot host",
				input: "polkadot://currenthost.dot/mytestapp.dot",
				expected: dot("currenthost.dot", "mytestapp.dot"),
			},
			TestCase {
				name: "pocket add deeplink",
				input: "polkadotapp://pocket/add?product=game.dot&card=loyalty",
				expected: pocket("game.dot", PocketDeeplinkAction::Add, "loyalty"),
			},
			TestCase {
				name: "pocket open deeplink with mixed case and arguments in either order",
				input: "polkadotapp://Pocket/open?card=loyalty&product=Game.DOT",
				expected: pocket("game.dot", PocketDeeplinkAction::Open, "loyalty"),
			},
			TestCase {
				name: "pocket deeplink without a card is rejected",
				input: "polkadotapp://pocket/add?product=game.dot",
				expected: Expected::Reject,
			},
			TestCase {
				name: "pocket deeplink without a product is rejected",
				input: "polkadotapp://pocket/add?card=loyalty",
				expected: Expected::Reject,
			},
			TestCase {
				name: "pocket deeplink naming a product outside dotNS is rejected",
				input: "polkadotapp://pocket/add?product=example.com&card=loyalty",
				expected: Expected::Reject,
			},
			TestCase {
				name: "pocket deeplink naming a localhost product is rejected",
				input: "polkadotapp://pocket/add?product=localhost:3000&card=loyalty",
				expected: Expected::Reject,
			},
			TestCase {
				name: "pocket deeplink without an action opens the collection",
				input: "polkadotapp://pocket",
				expected: pocket_collection(),
			},
			TestCase {
				name: "pocket collection deeplink takes no arguments",
				input: "polkadotapp://pocket/?product=game.dot&card=loyalty",
				expected: pocket_collection(),
			},
			TestCase {
				name: "a pocket action this core does not serve is rejected",
				input: "polkadotapp://pocket/frobnicate?product=game.dot&card=loyalty",
				expected: Expected::Reject,
			},
			TestCase {
				name: "pocket under a web scheme is an ordinary host",
				input: "https://pocket/add?product=game.dot&card=loyalty",
				expected: external("https://pocket/add?product=game.dot&card=loyalty"),
			},
			TestCase {
				name: "pocket under the product scheme is not the pocket",
				input: "polkadot://pocket/add?product=game.dot&card=loyalty",
				expected: Expected::Reject,
			},
			TestCase {
				name: "a product's own pocket path is an app route",
				input: "polkadot://game.dot/-/pocket/add?card=loyalty",
				expected: dot("game.dot", "-/pocket/add?card=loyalty"),
			},
			TestCase {
				name: "polkadotapp scheme hands off to the app",
				input: "polkadotapp://pay",
				expected: external("polkadotapp://pay"),
			},
			TestCase {
				name: "polkadot scheme with path",
				input: "polkadot://currenthost.dot/mytestapp.dot/settings",
				expected: dot("currenthost.dot", "mytestapp.dot/settings"),
			},
			TestCase {
				name: "polkadot scheme with query and hash",
				input: "polkadot://currenthost.dot/mytestapp.dot?embed=1#frame=compact",
				expected: dot("currenthost.dot", "mytestapp.dot?embed=1#frame=compact"),
			},
			TestCase {
				name: "dot subdomain",
				input: "sub.acme.dot/path",
				expected: dot("sub.acme.dot", "path"),
			},
			TestCase {
				name: "dot mixed case",
				input: "Example.DOT/Path",
				expected: dot("example.dot", "Path"),
			},
			TestCase {
				name: "dot with port is rejected",
				input: "https://x.dot:8080/path",
				expected: Expected::Reject,
			},
			TestCase {
				name: "dot with userinfo is rejected",
				input: "https://user:pass@x.dot/path",
				expected: Expected::Reject,
			},
			TestCase {
				name: "paseo bare",
				input: "mytestapp.paseo",
				expected: dot("mytestapp.paseo", ""),
			},
			TestCase {
				name: "paseo with path query hash",
				input: "pr508.faucet.paseo/nested/path?embed=1#frame=compact",
				expected: dot("pr508.faucet.paseo", "nested/path?embed=1#frame=compact"),
			},
			TestCase {
				name: "paseo mixed case",
				input: "Example.PASEO/Path",
				expected: dot("example.paseo", "Path"),
			},
			TestCase {
				name: "polkadot scheme paseo host",
				input: "polkadot://currenthost.paseo/mytestapp.paseo",
				expected: dot("currenthost.paseo", "mytestapp.paseo"),
			},
			TestCase {
				name: "paseo with port is rejected",
				input: "https://x.paseo:8443/path",
				expected: Expected::Reject,
			},
			TestCase {
				name: "paseo with userinfo is rejected",
				input: "https://user:pass@x.paseo/path",
				expected: Expected::Reject,
			},
			TestCase {
				name: "testnet bare",
				input: "browse.testnet",
				expected: dot("browse.testnet", ""),
			},
			TestCase {
				name: "testnet mixed case with path",
				input: "Browse.TESTNET/Path",
				expected: dot("browse.testnet", "Path"),
			},
			TestCase {
				name: "polkadot scheme testnet host",
				input: "polkadot://currenthost.testnet/browse.testnet",
				expected: dot("currenthost.testnet", "browse.testnet"),
			},
			TestCase {
				name: "testnet with port is rejected",
				input: "https://x.testnet:8443/path",
				expected: Expected::Reject,
			},
			TestCase {
				name: "testnet with userinfo is rejected",
				input: "https://user:pass@x.testnet/path",
				expected: Expected::Reject,
			},
			TestCase {
				name: "trim whitespace",
				input: "  mytestapp.dot/path  ",
				expected: dot("mytestapp.dot", "path"),
			},
			TestCase {
				name: "localhost bare with port",
				input: "localhost:3000",
				expected: localhost("localhost:3000", ""),
			},
			TestCase {
				name: "localhost with port and path",
				input: "localhost:3000/some/path",
				expected: localhost("localhost:3000", "some/path"),
			},
			TestCase {
				name: "localhost with explicit http",
				input: "http://localhost:5000",
				expected: localhost("localhost:5000", ""),
			},
			TestCase {
				name: "localhost with http and path",
				input: "http://localhost:5000/path",
				expected: localhost("localhost:5000", "path"),
			},
			TestCase {
				name: "localhost with query and hash",
				input: "localhost:3000/path?q=1#h",
				expected: localhost("localhost:3000", "path?q=1#h"),
			},
			TestCase {
				name: "localhost without port",
				input: "localhost",
				expected: localhost("localhost", ""),
			},
			TestCase {
				name: "localhost without port with path",
				input: "localhost/path",
				expected: localhost("localhost", "path"),
			},
			TestCase {
				name: "external bare domain",
				input: "google.com",
				expected: external("https://google.com/"),
			},
			TestCase {
				name: "external bare domain with path",
				input: "google.com/search?q=test",
				expected: external("https://google.com/search?q=test"),
			},
			TestCase {
				name: "external preserves https",
				input: "https://example.com/page",
				expected: external("https://example.com/page"),
			},
			TestCase {
				name: "external preserves http",
				input: "http://example.com/page",
				expected: external("http://example.com/page"),
			},
			TestCase {
				name: "external dot li",
				input: "acme.dot.li/path/1",
				expected: external("https://acme.dot.li/path/1"),
			},
			TestCase {
				name: "external messages handoff",
				input: "sms:+15551234567",
				expected: external("sms:+15551234567"),
			},
			TestCase {
				name: "external maps handoff",
				input: "maps:?q=Berlin",
				expected: external("maps:?q=Berlin"),
			},
			TestCase { name: "reject empty", input: "", expected: Expected::Reject },
			TestCase { name: "reject whitespace", input: "   ", expected: Expected::Reject },
			TestCase {
				name: "reject unparseable",
				input: ":::invalid",
				expected: Expected::Reject,
			},
			TestCase {
				name: "reject javascript URI",
				input: "javascript:alert(1)",
				expected: Expected::Reject,
			},
			TestCase {
				name: "reject file URI",
				input: "file:///etc/passwd",
				expected: Expected::Reject,
			},
			TestCase {
				name: "reject data URI",
				input: "data:text/html,<script>alert(1)</script>",
				expected: Expected::Reject,
			},
			TestCase {
				name: "reject vbscript URI",
				input: "vbscript:msgbox(1)",
				expected: Expected::Reject,
			},
		];

		for case in cases {
			let actual = parse_navigate(case.input);
			match case.expected {
				Expected::Decision(expected) => assert_eq!(actual, expected, "{}", case.name),
				Expected::Reject => {
					assert!(
						matches!(actual, NavigateDecision::Reject { .. }),
						"{}: expected Reject, got {actual:?}",
						case.name,
					)
				},
			}
		}

		let nfc = parse_navigate("café.dot");
		let nfd = parse_navigate("cafe\u{0301}.dot");
		match (&nfc, &nfd) {
			(
				NavigateDecision::DotName { identifier: a, .. },
				NavigateDecision::DotName { identifier: b, .. },
			) => assert_eq!(a, b, "NFC and NFD inputs must normalize to one identifier"),
			other => panic!("expected two DotName decisions, got {other:?}"),
		}
	}

	#[test]
	fn pocket_deeplinks_screen_the_card_id_the_way_removal_does() {
		// A deeplink and `remove_card` name the same card, so they have to
		// agree on what the name is. Removal normalizes, so navigation does
		// too, or `?card=%20loyalty%20` adds a card that cannot be removed.
		match parse_navigate("polkadotapp://pocket/add?product=game.dot&card=%20loyalty%20") {
			NavigateDecision::Pocket { card_id, canonical_url, .. } => {
				assert_eq!(card_id, "loyalty");
				assert_eq!(
					canonical_url, "polkadotapp://pocket/add?product=game.dot&card=loyalty",
					"the canonical form carries the normalized id"
				);
			},
			other => panic!("expected a Pocket decision, got {other:?}"),
		}

		// NFD and NFC spellings of one name must not be two cards.
		let nfc = parse_navigate("polkadotapp://pocket/open?product=game.dot&card=caf%C3%A9");
		let nfd = parse_navigate("polkadotapp://pocket/open?product=game.dot&card=cafe%CC%81");
		match (&nfc, &nfd) {
			(
				NavigateDecision::Pocket { card_id: a, .. },
				NavigateDecision::Pocket { card_id: b, .. },
			) => assert_eq!(a, b, "NFC and NFD spellings name one card"),
			other => panic!("expected two Pocket decisions, got {other:?}"),
		}

		// The bounds chat applies to its own identifiers apply here too.
		let too_long = "a".repeat(257);
		for input in [
			format!("polkadotapp://pocket/add?product=game.dot&card={too_long}"),
			// A zero-width joiner lets two distinct ids render identically.
			"polkadotapp://pocket/add?product=game.dot&card=loy%E2%80%8Dalty".to_string(),
			"polkadotapp://pocket/add?product=game.dot&card=%20%20".to_string(),
		] {
			assert!(
				matches!(parse_navigate(&input), NavigateDecision::Reject { .. }),
				"{input} should be refused"
			);
		}
	}

	#[test]
	fn pocket_canonical_url_reencodes_the_card_id() {
		// `card` arrives percent-decoded, so a card id carrying URL syntax
		// would otherwise re-parse as a different deeplink: `#` starts a
		// fragment and `&` starts a second parameter.
		for (input, expected) in [
			(
				"polkadotapp://pocket/add?product=game.dot&card=a%23b",
				"polkadotapp://pocket/add?product=game.dot&card=a%23b",
			),
			(
				"polkadotapp://pocket/open?product=game.dot&card=a%26open%3Dx",
				"polkadotapp://pocket/open?product=game.dot&card=a%26open%3Dx",
			),
		] {
			match parse_navigate(input) {
				NavigateDecision::Pocket { canonical_url, card_id, .. } => {
					assert_eq!(canonical_url, expected, "canonical url for {input}");
					// Re-parsing the canonical form has to name the same card.
					match parse_navigate(&canonical_url) {
						NavigateDecision::Pocket { card_id: again, .. } => {
							assert_eq!(again, card_id, "round trip for {input}")
						},
						other => panic!("canonical url stopped being a Pocket target: {other:?}"),
					}
				},
				other => panic!("expected a Pocket decision for {input}, got {other:?}"),
			}
		}
	}
}
