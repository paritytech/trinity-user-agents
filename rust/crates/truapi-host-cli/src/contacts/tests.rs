use super::*;

#[test]
fn a_spec_names_a_contact_and_its_account() {
	let contacts =
		parse_contacts("alice=0x0101010101010101010101010101010101010101010101010101010101010101");
	assert_eq!(contacts.len(), 1);
	assert_eq!(contacts[0].display_name.as_deref(), Some("alice"));
	assert_eq!(contacts[0].account, [1u8; 32]);
}

#[test]
fn an_account_that_is_not_32_bytes_is_not_a_contact() {
	// Dropped rather than padded: a short account is a typo, and padding it
	// would mint a handle for somebody who does not exist.
	assert!(parse_contacts("alice=0x0102").is_empty());
	assert!(parse_contacts("alice=not-hex").is_empty());
	assert!(parse_contacts("no-account").is_empty());
}

#[test]
fn an_empty_spec_is_an_empty_list_rather_than_the_development_one() {
	assert!(parse_contacts("").is_empty());
	assert_eq!(development_contacts().len(), 2);
}

#[test]
fn the_development_accounts_are_distinct() {
	let contacts = development_contacts();
	assert_ne!(contacts[0].account, contacts[1].account);
}

#[test]
fn the_usernames_are_the_configured_names_in_order() {
	// Given
	let host = CliContactsHost {
		contacts: parse_contacts(
			"alice=0x0101010101010101010101010101010101010101010101010101010101010101;\
			 bob.07=0x0202020202020202020202020202020202020202020202020202020202020202",
		),
		platform: CliPlatform::new(
			&crate::network::Network::default().bundled_stack(crate::chain::ChainBackend::Rpc),
			None,
			crate::platform::ApprovalPolicy::AutoAccept,
			None,
		),
	};

	// When
	let listing = futures::executor::block_on(host.contact_usernames(&ProductContext::new(
		truapi::platform::ProductId::for_test("dim2.dot"),
	)));

	// Then
	let Ok(HostContactUsernames::Listed { usernames }) = listing else {
		panic!("the CLI host did not list usernames");
	};
	assert_eq!(usernames, ["alice", "bob.07"]);
}
