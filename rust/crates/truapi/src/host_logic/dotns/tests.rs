use super::*;

/// The stack TLD the cases run on unless they name another.
const STACK_TLD: &str = "paseo";

fn navigate(input: &str) -> NavigateDecision {
	parse_navigate(input, STACK_TLD)
}

#[test]
fn product_links_require_the_explicit_network_for_direct_and_share_forms() {
	let suffixes = ["dot", "paseo", "fork"];
	for suffix in suffixes {
		for mirror in ["", ".li"] {
			let input = format!("https://app.arena.coinflip.{suffix}{mirror}/play?mode=1#turn");
			assert_eq!(
				ProductLink::parse(&input, suffix),
				Some(ProductLink {
					identifier: ProductId::for_test(&format!("app.arena.coinflip.{suffix}")),
					path: "/play?mode=1#turn".to_string(),
					canonical_url: format!("https://app.arena.coinflip.{suffix}/play?mode=1#turn"),
				})
			);
			for other in suffixes.into_iter().filter(|other| *other != suffix) {
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
		("polkadotapp://browse.dot/onboarding?ref=abc#intro", "/onboarding?ref=abc#intro"),
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
		"file://browse.dot",
		"otherwallet://browse.dot",
		"polkadotapp://pocket",
		"https://invalid_name.dot.li",
		"https://browse.dot.li..",
		"https://dot.li",
		"https://stg.revx.dev/editor?mod=other.dot",
	] {
		assert_eq!(ProductLink::parse(input, "dot"), None, "{input}");
	}
}

#[test]
fn share_hosts_are_the_reserved_tld_and_the_stacks_own() {
	assert_eq!(product_share_hosts("paseo"), ["dot.li", "paseo.li"]);
	assert_eq!(product_share_hosts("fork"), ["dot.li", "fork.li"]);
	assert_eq!(product_share_hosts("dot"), ["dot.li"]);
}

/// A name on a network whose TLD is the share suffix is that network's
/// product, not a share link to a name one label shorter.
#[test]
fn a_network_on_the_share_suffix_keeps_its_own_names() {
	assert_eq!(
		ProductLink::parse("https://app.arena.li/play", "li").map(|link| link.identifier),
		Some(ProductId::for_test("app.arena.li"))
	);
}

/// A fork runs products under its own TLD and under Polkadot's, and reads any
/// other network's names as the web addresses they are.
#[test]
fn a_host_reads_only_its_own_and_the_reserved_tld_as_dotns() {
	for (input, stack_tld, is_dotns) in [
		("browse.fork", "fork", true),
		("browse.dot", "fork", true),
		("browse.paseo", "fork", false),
		("browse.testnet", "paseo", false),
		("browse.dot", "paseo", true),
		("browse.paseo", "dot", false),
	] {
		let expected = if is_dotns {
			NavigateDecision::DotName {
				identifier: ProductId::for_test(input),
				path: String::new(),
				canonical_url: format!("polkadot://{input}"),
			}
		} else {
			NavigateDecision::External { url: format!("https://{input}/") }
		};
		assert_eq!(parse_navigate(input, stack_tld), expected, "{input} on a {stack_tld} stack");
	}
}

/// A Pocket deeplink is the host's own, so it is refused rather than opened
/// elsewhere when its product is on a TLD the stack does not serve.
#[test]
fn a_pocket_deeplink_names_a_product_the_stack_serves() {
	let link = |product: &str| format!("polkadotapp://pocket/add?product={product}&card=loyalty");
	for (product, stack_tld, served) in [
		("game.fork", "fork", true),
		("game.dot", "fork", true),
		("game.paseo", "fork", false),
		("game.testnet", "paseo", false),
	] {
		let decision = parse_navigate(&link(product), stack_tld);
		assert_eq!(
			matches!(decision, NavigateDecision::Pocket { .. }),
			served,
			"{product} on a {stack_tld} stack: {decision:?}"
		);
		assert!(served || matches!(decision, NavigateDecision::Reject { .. }), "{decision:?}");
	}
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
		canonical_url: join_url("polkadot", identifier, path),
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
		page_query: String::new(),
		canonical_url: format!("polkadotapp://pocket/{verb}?product={identifier}&card={card_id}"),
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
		canonical_url: join_url("http", host, path),
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
		assert!(matches!(navigate(input), NavigateDecision::Reject { .. }), "{input}");
	}
}

#[test]
fn a_product_destination_is_handed_over_as_a_polkadot_url() {
	for (input, expected) in [
		("calculator.paseo", "polkadot://calculator.paseo"),
		("polkadot://calculator.paseo", "polkadot://calculator.paseo"),
		("https://calculator.paseo", "polkadot://calculator.paseo"),
		("calculator.paseo/deep/path?q=1#frag", "polkadot://calculator.paseo/deep/path?q=1#frag"),
		("MyTestApp.DOT", "polkadot://mytestapp.dot"),
	] {
		let NavigateDecision::DotName { canonical_url, .. } = navigate(input) else {
			panic!("{input} should classify as a dotNS product destination");
		};
		assert_eq!(canonical_url, expected, "for {input}");
	}
}

/// A web address keeps its scheme, so the two stay distinguishable.
#[test]
fn a_web_address_is_still_handed_over_as_https() {
	let NavigateDecision::External { url } = navigate("https://example.com/x") else {
		panic!("example.com should classify as external");
	};
	assert_eq!(url, "https://example.com/x");
}

#[test]
fn parse_navigate_cases() {
	let cases = vec![
		TestCase { name: "dot bare", input: "mytestapp.dot", expected: dot("mytestapp.dot", "") },
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
		TestCase { name: "reject unparseable", input: ":::invalid", expected: Expected::Reject },
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
		let actual = navigate(case.input);
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

	let nfc = navigate("café.dot");
	let nfd = navigate("cafe\u{0301}.dot");
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
	match navigate("polkadotapp://pocket/add?product=game.dot&card=%20loyalty%20") {
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
	let nfc = navigate("polkadotapp://pocket/open?product=game.dot&card=caf%C3%A9");
	let nfd = navigate("polkadotapp://pocket/open?product=game.dot&card=cafe%CC%81");
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
		format!("polkadotapp://pocket/open?product=game.dot&card=a&v={}", "a".repeat(2048)),
		"polkadotapp://pocket/open?product=game.dot&card=a&v={x}".to_string(),
		"polkadotapp://pocket/open?product=game.dot&card=a&v=1%".to_string(),
		"polkadotapp://pocket/open?product=game.dot&card=a&v=%+1".to_string(),
	] {
		assert!(
			matches!(navigate(&input), NavigateDecision::Reject { .. }),
			"{input} should be refused"
		);
	}
}

#[test]
fn pocket_canonical_url_reencodes_the_card_id() {
	// `card` arrives percent-decoded, so a card id carrying URL syntax
	// would otherwise re-parse as a different deeplink: `#` starts a
	// fragment and `&` starts a second parameter. Page params stay as
	// written, and `add` loads no page, so it drops them.
	for (input, expected) in [
		(
			"polkadotapp://pocket/add?product=game.dot&card=a%23b&open=x",
			"polkadotapp://pocket/add?product=game.dot&card=a%23b",
		),
		(
			"polkadotapp://pocket/open?product=game.dot&card=a%26open%3Dx",
			"polkadotapp://pocket/open?product=game.dot&card=a%26open%3Dx",
		),
		(
			"polkadotapp://pocket/open?v=a+b%26c&card=x&product=game.dot&card=y&flag",
			"polkadotapp://pocket/open?product=game.dot&card=x&v=a+b%26c&flag",
		),
	] {
		match navigate(input) {
			NavigateDecision::Pocket { canonical_url, card_id, .. } => {
				assert_eq!(canonical_url, expected, "canonical url for {input}");
				// Re-parsing the canonical form has to name the same card.
				match navigate(&canonical_url) {
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
