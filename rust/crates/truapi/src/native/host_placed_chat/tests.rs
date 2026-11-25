use super::*;

#[test]
fn every_host_places_jollity_on_its_own_network() {
	assert_eq!(
		host_placed_chats("paseo".into()).unwrap(),
		vec![HostPlacedChat {
			product_id: ProductId::for_test("dim2.paseo"),
			room_id: "jollity".into(),
			name: "Jollity".into(),
		}]
	);
}

#[test]
fn a_chat_needs_a_network_its_product_can_be_named_on() {
	assert!(host_placed_chats("com".into()).is_err());
}
