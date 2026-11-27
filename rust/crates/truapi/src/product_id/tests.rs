use super::*;
use crate::dotns_name::MAX_DNS_LABEL_LEN;

// Hosts store rooms under this id, so a change to the derivation loses
// every stored room.
#[test]
fn a_chat_room_id_is_stable_for_one_product_and_source() {
	assert_eq!(
		ProductId::for_test("chat.dot").chat_room_id("support"),
		"0x4fb691cd1a1d7eb11cfd0b127b02f995c63329cad7e0fbcf470129d592d64d75"
	);
}

#[test]
fn a_chat_room_id_is_unique_to_its_product_and_source() {
	let chat = ProductId::for_test("chat.dot");

	assert_ne!(
		chat.chat_room_id("support"),
		ProductId::for_test("shop.dot").chat_room_id("support")
	);
	assert_ne!(chat.chat_room_id("support"), chat.chat_room_id("sales"));
	// Without the length prefixes, both pairs give `localhost:3000a`.
	assert_ne!(
		ProductId::for_test("localhost:300").chat_room_id("0a"),
		ProductId::for_test("localhost:3000").chat_room_id("a")
	);
}

#[test]
fn named_products_preserve_their_full_namespace() {
	let identifier = ProductId::from_name("App.Arena.Coinflip", "paseo").unwrap();
	assert_eq!(
		(identifier.clone(), identifier.dotns_name(), identifier.network_suffix()),
		(
			ProductId::for_test("app.arena.coinflip.paseo"),
			Some("app.arena.coinflip"),
			Some("paseo")
		)
	);
	assert!(identifier.matches_network("paseo"));
	assert!(!identifier.matches_network("dot"));
}

#[test]
fn named_products_take_one_label_as_the_network_suffix() {
	// A dotted suffix would move labels of the name into the network.
	for suffix in ["", "dot.li", "other.dot", "DOT", "-dot"] {
		assert!(ProductId::from_name("browse", suffix).is_err(), "{suffix}");
	}
	assert_eq!(ProductId::from_name("browse", "fork"), Ok(ProductId::for_test("browse.fork")));
	let localhost = ProductId::for_test("localhost:3000");
	assert_eq!((localhost.dotns_name(), localhost.network_suffix()), (None, None));
	assert!(!localhost.matches_network("dot"));
}

/// Whether `raw` parses to an id with a trusted label. The stack tests
/// cover the TLD.
fn has_trusted_label(raw: &str) -> bool {
	raw.parse::<ProductId>().is_ok_and(|id| id.has_trusted_remote_permissions())
}

#[test]
fn trusted_remote_permission_labels_match_the_bare_product_label() {
	for product_id in [
		"peopl.dot",
		"peopl.paseo",
		"peopl.testnet",
		"dim2.dot",
		"dim2.paseo",
		"dim2.testnet",
		"stash.dot",
	] {
		assert!(
			has_trusted_label(product_id),
			"{product_id} must hold remote permissions without a prompt"
		);
	}
	for product_id in [
		"app.peopl.dot",
		"sub.dim2.paseo",
		"app.dim2.testnet",
		"notdim2.dot",
		"peopl",
		"peoplx.dot",
		"my-peopl.dot",
		"localhost",
		"localhost:3000",
		"",
		"dot",
	] {
		assert!(
			!has_trusted_label(product_id),
			"{product_id} is a separate product and must prompt"
		);
	}
}

#[test]
fn a_published_product_cannot_take_the_localhost_label() {
	// Hosts grant a development wildcard to a localhost product, so the
	// label must not be reachable from dotNS.
	for product_id in ["localhost.dot", "localhost.paseo", "localhost.fork", "LOCALHOST.DOT"] {
		assert!(
			product_id.parse::<ProductId>().is_err(),
			"{product_id} must not be a valid product identifier"
		);
		assert!(
			!is_localhost_form(product_id),
			"{product_id} must not read as the development wildcard"
		);
	}
	for product_id in ["localhost", "localhost:3000"] {
		assert!(is_localhost_form(product_id), "{product_id}");
	}
	// A prefix test would admit these; the whole identifier has to match.
	for product_id in ["localhost:3000.evil.dot", "localhost:", "localhost:80a"] {
		assert!(
			!is_localhost_form(product_id),
			"{product_id} must not read as the development wildcard"
		);
		assert!(
			product_id.parse::<ProductId>().is_err(),
			"{product_id} must not pass as a dotNS name either"
		);
	}
	// Still a normal product, sharing only the prefix.
	assert!("localhosting.dot".parse::<ProductId>().is_ok());
}

#[test]
fn trusted_remote_permissions_normalize_the_spellings_a_host_holds() {
	// Both the UniFFI and wasm exports parse before asking, and a host
	// holds an id in whatever spelling it received rather than the
	// normalized form the core passes internally.
	for product_id in
		["peopl.dot", "PEOPL.DOT", "  peopl.dot  ", "dim2.paseo", " DIM2.TESTNET ", "stash.dot"]
	{
		assert!(
			has_trusted_label(product_id),
			"{product_id} is a first-party product in any accepted spelling"
		);
	}
	for product_id in ["app.peopl.dot", "peopl", "notpeopl.dot", "localhost:3000", "", "   "] {
		assert!(!has_trusted_label(product_id), "{product_id} must not be read as trusted");
	}
}

/// Parsing has no stack to ask, so it accepts any TLD dotNS could register.
/// The stack decides which TLDs it serves.
#[test]
fn any_dns_label_is_a_tld_and_a_name_needs_one() {
	for id in ["browse.fork", "browse.com", "app.browse.x1"] {
		assert!(id.parse::<ProductId>().is_ok(), "{id:?} is well formed");
	}
	for id in ["browse", "browse.", "browse.-x", "browse.my_tld"] {
		assert!(id.parse::<ProductId>().is_err(), "{id:?} has no well-formed TLD");
	}
}

#[test]
fn an_identifier_carrying_no_name_is_rejected() {
	// Each of these would reach the manifest grant key, the cache key, the
	// only consent prompt in this design and every log line.
	for id in [
		"dim2\u{0}.dot",        // NUL, into a prompt rendered verbatim
		"\u{202e}dim2.dot",     // right-to-left override
		"dim2\u{200b}.dot",     // zero width space
		"a b c.dot",            // whitespace
		"../../etc/passwd.dot", // path separators
		"dim2..dot",            // empty label: collapses onto other ids
		".dot",                 // same empty label
	] {
		assert!(
			id.parse::<ProductId>().is_err(),
			"{id:?} carries no product name and must not normalize"
		);
	}
}

#[test]
fn only_a_name_dotns_can_register_is_an_identifier() {
	let longest_label = "a".repeat(MAX_DNS_LABEL_LEN);
	for id in ["Dim2.DOT", "dim2-two.dot", "0x.dot", &format!("{longest_label}.dot")] {
		assert!(id.parse::<ProductId>().is_ok(), "{id:?} is a name dotNS can register");
	}
	// dotNS refuses each of these at registration, so none can ever resolve.
	for id in [
		"tést.dot",
		"münchen.dot",
		"my_app.dot",
		"-dim2.dot",
		"dim2-.dot",
		"\u{ff44}\u{ff49}\u{ff4d}2.dot",
		"dim2\u{ad}.dot",
		&format!("{longest_label}a.dot"),
	] {
		assert!(id.parse::<ProductId>().is_err(), "{id:?} is not a name dotNS can register");
	}
}

#[test]
fn an_overlong_product_id_is_not_an_identifier() {
	// Every label is well formed, so without the cap any length would be
	// a valid, distinct cache key taken from the wire.
	let full_label = "a".repeat(MAX_DNS_LABEL_LEN);
	let at_cap = format!("{full_label}.{full_label}.{full_label}.{}.dot", "a".repeat(60));
	assert_eq!(at_cap.len(), PRODUCT_ID_MAX_BYTES);
	assert!(at_cap.parse::<ProductId>().is_ok(), "an id exactly at the cap is still valid");

	let overlong = format!("{full_label}.{full_label}.{full_label}.{}.dot", "a".repeat(61));
	assert!(
		overlong.parse::<ProductId>().is_err(),
		"a product id past the cap must be rejected, not stored"
	);
}

#[test]
fn persisted_json_is_a_canonical_string_and_validates_on_restore() {
	let id = ProductId::for_test(" App.Peopl.DOT ");
	assert_eq!(serde_json::to_string(&id).unwrap(), r#""app.peopl.dot""#);
	assert_eq!(serde_json::from_str::<ProductId>(r#"" App.Peopl.DOT ""#).unwrap(), id);
	assert!(serde_json::from_str::<ProductId>(r#""not_a.dot""#).is_err());
}

#[test]
fn the_bare_label_drops_the_tld() {
	assert_eq!(ProductId::for_test("dim2.dot").bare_label(), "dim2");
	assert_eq!(ProductId::for_test("dim2.paseo").bare_label(), "dim2");
	assert_eq!(ProductId::for_test("localhost").bare_label(), "localhost");
}

#[test]
fn an_executable_carries_the_label_of_the_product_it_belongs_to() {
	assert_eq!(ProductId::for_test("app.dim2.dot").bare_label(), "dim2");
	assert_eq!(ProductId::for_test("widget.dim2.dot").bare_label(), "dim2");
	assert_eq!(ProductId::for_test("worker.dim2.paseo").bare_label(), "dim2");
	assert_eq!(ProductId::for_test("funding.dim2.dot").bare_label(), "dim2");
}

#[test]
fn a_subname_of_another_domain_takes_that_domains_label() {
	assert_eq!(ProductId::for_test("dim2.attacker.dot").bare_label(), "attacker");
}

#[test]
fn one_product_on_one_network_is_its_executables_under_one_tld() {
	assert!(
		ProductId::for_test("app.dim2.dot")
			.same_product_on_one_network(&ProductId::for_test("dim2.dot"))
	);
	assert!(
		!ProductId::for_test("dim2.paseo")
			.same_product_on_one_network(&ProductId::for_test("dim2.dot"))
	);
	assert!(
		!ProductId::for_test("dim2.attacker.dot")
			.same_product_on_one_network(&ProductId::for_test("dim2.dot"))
	);
}

#[test]
fn decoding_normalizes_and_rejects_what_parsing_rejects() {
	let frame = "DIM2.Dot".to_string().encode();
	assert_eq!(ProductId::decode(&mut frame.as_slice()), Ok(ProductId::for_test("dim2.dot")));
	let frame = "localhost.dot".to_string().encode();
	assert!(ProductId::decode(&mut frame.as_slice()).is_err());
}

#[test]
fn a_share_host_is_read_by_its_syntax_alone() {
	for (input, expected) in [
		("BROWSE.TESTNET.LI", Some("browse.testnet")),
		("browse.test.li", Some("browse.test")),
		("browse.dot", None),
		("https://browse.dot.li", None),
		("localhost.li", None),
	] {
		assert_eq!(ProductId::from_share_host(input), expected.map(ProductId::for_test), "{input}");
	}
}

#[test]
fn share_hosts_preserve_the_product_network() {
	for (input, expected) in [
		("browse.dot", Some("browse.dot.li")),
		("browse.paseo", Some("browse.paseo.li")),
		("widget.browse.testnet", Some("widget.browse.testnet.li")),
		("localhost", None),
		("localhost:3000", None),
	] {
		assert_eq!(ProductId::for_test(input).to_share_host().as_deref(), expected);
	}
}

#[test]
fn executable_owners_preserve_nested_product_names() {
	let owner = ProductId::for_test("arena.coinflip.paseo");
	for (kind, expected) in [
		(ExecutableKind::App, "app.arena.coinflip.paseo"),
		(ExecutableKind::Widget, "widget.arena.coinflip.paseo"),
		(ExecutableKind::Worker, "worker.arena.coinflip.paseo"),
	] {
		let executable = owner.executable(kind).unwrap();
		assert_eq!(executable, ProductId::for_test(expected));
		assert_eq!(executable.executable_owner().unwrap(), owner);
	}
}

#[test]
fn only_an_executable_prefix_with_a_complete_owner_is_removed() {
	for (input, expected) in [
		("app.dot", "app.dot"),
		("arena.coinflip.dot", "arena.coinflip.dot"),
		("app.worker.coinflip.dot", "worker.coinflip.dot"),
		("localhost:3000", "localhost:3000"),
	] {
		assert_eq!(
			ProductId::for_test(input).executable_owner().unwrap(),
			ProductId::for_test(expected)
		);
	}
}

#[test]
fn executable_names_cannot_create_invalid_product_ids() {
	assert!(ProductId::for_test("app.localhost.dot").executable_owner().is_err());
	assert!(ProductId::for_test("localhost:3000").executable(ExecutableKind::App).is_err());
	let label = "a".repeat(63);
	let longest = ProductId::for_test(&format!("{label}.{label}.{label}.{}.dot", "b".repeat(60)));
	assert!(longest.executable(ExecutableKind::Worker).is_err());
}
