use super::*;
use crate::host_internal::sso_messages::{
	CreateTransactionRequest, CreateTransactionWithLegacyAccountRequest,
	SignRawWithLegacyAccountRequest,
};

#[test]
fn identical_payloads_keep_their_request_specific_response_variants() {
	let response = Response { responding_to: "m-1".to_string(), payload: Ok(vec![7]) };
	let transaction = CreateTransactionRequest::response_into_message(response.clone());
	assert_eq!(transaction.name(), "CreateTransactionResponse");
	assert_eq!(
		CreateTransactionWithLegacyAccountRequest::response_from_message(transaction.clone()),
		Some(response.clone()),
	);
	assert!(SignRawWithLegacyAccountRequest::response_from_message(transaction).is_none());

	let signature = SignRawWithLegacyAccountRequest::response_into_message(response);
	assert_eq!(signature.name(), "SignRawWithLegacyAccountResponse");
	assert!(CreateTransactionRequest::response_from_message(signature).is_none());
}
