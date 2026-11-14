use super::*;
use std::fs::read_to_string;

fn product() -> ProductContext {
	ProductContext::new(ProductId::for_test("pocket.dot"))
}

fn other_product() -> ProductContext {
	ProductContext::new(ProductId::for_test("other.dot"))
}

/// A card id is product-local, so one product must not see or remove
/// another's cards. The CLI serves every product from one process, and a
/// `/product` switch reuses the same host.
#[test]
fn one_product_cannot_see_or_remove_another_products_cards() {
	let host = CliPocketHost::new(
		BTreeMap::from([("loyalty".to_string(), false)]),
		Transcript::new(None, "pocket"),
	);
	let mine = product();
	let theirs = other_product();

	let removed =
		futures::executor::block_on(host.remove_pocket_card(
			&mine,
			HostPocketRemoveCardRequest { card_id: "loyalty".to_string() },
		));
	assert!(removed.is_ok());

	let mut theirs_cards = host.subscribe_pocket_cards(&theirs);
	let seen = futures::executor::block_on(theirs_cards.next())
		.expect("the current list arrives on subscribe")
		.expect("no stream error");
	assert_eq!(
		seen.cards,
		vec![v01::PocketCard { card_id: "loyalty".to_string(), privileged: false }],
		"the other product still holds its own copy"
	);
}

fn host(spec: &str) -> (Arc<CliPocketHost>, tempfile::NamedTempFile) {
	let transcript = tempfile::NamedTempFile::new().expect("a temp transcript");
	let host = CliPocketHost::new(
		parse_cards(spec),
		Transcript::new(Some(transcript.path().to_path_buf()), "pocket"),
	);
	(host, transcript)
}

fn remove(host: &CliPocketHost, card_id: &str) -> Result<(), HostPocketRemoveCardError> {
	futures::executor::block_on(host.remove_pocket_card(
		&product(),
		HostPocketRemoveCardRequest { card_id: card_id.to_string() },
	))
}

#[test]
fn the_card_spec_marks_privileged_cards() {
	let cards = parse_cards("loyalty, humanity:privileged ,");

	assert_eq!(cards.get("loyalty"), Some(&false));
	assert_eq!(cards.get("humanity"), Some(&true));
	assert_eq!(cards.len(), 2);
}

#[test]
fn removal_follows_the_protocol_rules_and_republishes_the_list() {
	let (host, transcript) = host("loyalty,humanity:privileged");
	let mut lists = host.subscribe_pocket_cards(&product());
	let first = futures::executor::block_on(lists.next())
		.expect("the snapshot arrives on subscribe")
		.expect("no stream error");
	assert_eq!(first.cards.len(), 2);

	assert!(matches!(remove(&host, "humanity"), Err(HostPocketRemoveCardError::Privileged)));
	assert!(remove(&host, "absent").is_ok());
	assert!(remove(&host, "loyalty").is_ok());

	let republished = futures::executor::block_on(lists.next())
		.expect("a removal republishes the list")
		.expect("no stream error");
	assert_eq!(republished.cards.len(), 1);
	assert!(republished.cards[0].privileged);

	// What the host itself observed, which is what a battery reads to tell
	// a refusal apart from a removal it never attempted.
	let kinds: Vec<String> = read_to_string(transcript.path())
		.expect("the transcript is readable")
		.lines()
		.map(|line| {
			serde_json::from_str::<serde_json::Value>(line).expect("one json object per line")
                ["kind"]
                .as_str()
                .unwrap_or_default()
                .to_string()
		})
		.collect();
	assert_eq!(kinds, ["remove_refused", "remove_absent", "removed"]);
}
