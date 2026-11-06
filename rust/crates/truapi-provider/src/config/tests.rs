use super::ChainSource;

#[test]
fn light_client_enables_statement_protocol_by_default() {
	let statement_protocol = match ChainSource::light_client("{}").build() {
		ChainSource::LightClient { statement_protocol, .. } => statement_protocol,
		#[cfg(feature = "ws")]
		ChainSource::RpcNode { .. } => panic!("expected a LightClient source"),
	};
	assert!(statement_protocol);
}

#[test]
fn builder_sets_database() {
	let source = ChainSource::light_client("{}").database("db").build();
	let database_content = match source {
		ChainSource::LightClient { database_content, .. } => database_content,
		#[cfg(feature = "ws")]
		ChainSource::RpcNode { .. } => panic!("expected a LightClient source"),
	};
	assert_eq!(database_content.as_deref(), Some("db"));
}
