use super::ChainSource;

#[test]
fn light_client_enables_statement_protocol_by_default() {
	let ChainSource::LightClient { statement_protocol, .. } =
		ChainSource::light_client("{}").build()
	else {
		panic!("expected a LightClient source");
	};
	assert!(statement_protocol);
}

#[test]
fn builder_sets_database() {
	let source = ChainSource::light_client("{}").database("db").build();
	let ChainSource::LightClient { database_content, .. } = source else {
		panic!("expected a LightClient source");
	};
	assert_eq!(database_content.as_deref(), Some("db"));
}
