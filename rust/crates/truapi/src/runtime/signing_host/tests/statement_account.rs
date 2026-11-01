//! The account a conversation's allowance claim funds is the one the product
//! signs with: its own account at the raw index it named, never another
//! product's and never one the caller chose.

use super::*;

use truapi::{
	api::StatementStore,
	latest,
	versioned::statement_store::{
		RemoteStatementStoreCreateProofRequest, RemoteStatementStoreCreateProofResponse,
	},
};

const INDEX: [u8; 32] = [0x5b; 32];

fn signer_of_create_proof(
	services: Arc<RuntimeServices>,
	authority: Arc<SigningHostRole>,
	product: &str,
	index: [u8; 32],
) -> [u8; 32] {
	let runtime = product_runtime_for(services, authority, product);
	let response = futures::executor::block_on(runtime.create_proof(
		&CallContext::default(),
		RemoteStatementStoreCreateProofRequest::V1(
			latest::RemoteStatementStoreCreateProofRequest {
				product_account_id: latest::ProductAccountId {
					dot_ns_identifier: ProductId::for_test(product),
					derivation_index: latest::DerivationIndex::Raw(index),
				},
				statement: crate::test_support::statement(),
			},
		),
	))
	.expect("a product signs with its own account");
	let RemoteStatementStoreCreateProofResponse::V1(response) = response;
	let latest::StatementProof::Sr25519 { signer, .. } = response.proof else {
		panic!("expected an sr25519 proof");
	};
	signer
}

#[test]
fn the_claimed_account_is_the_one_create_proof_signs_with_for_that_product_and_index() {
	let (services, authority) = signing_runtime();
	futures::executor::block_on(authority.activate_local_session(ENTROPY.to_vec()))
		.expect("the local session activates");
	let claimed = |product: &str, index: [u8; 32]| {
		authority
			.product_statement_account(&ProductId::for_test(product), index)
			.expect("the account derives")
	};

	assert_eq!(
		claimed("chat.dot", INDEX),
		signer_of_create_proof(services.clone(), authority.clone(), "chat.dot", INDEX),
	);
	// Another product, or another index, is another account: a product funds
	// only its own, one per index.
	assert_ne!(claimed("chat.dot", INDEX), claimed("other.dot", INDEX));
	assert_ne!(claimed("chat.dot", INDEX), claimed("chat.dot", [0x5c; 32]));
}

/// Without a session the claim derives nothing and reads no chain, so a
/// logout before the call leaves nothing to submit.
#[test]
fn a_claim_without_a_session_reads_and_submits_nothing() {
	let platform = Arc::new(StubPlatform::default());
	let (_services, authority) = signing_runtime_with_platform(platform.clone());

	let outcome = futures::executor::block_on(authority.claim_product_statement_account(
		&ProductId::for_test("chat.dot"),
		INDEX,
		crate::runtime::statement_allowance::ClaimSubmission::Submit,
	));

	assert_eq!(
		(
			outcome.map(|_| ()).map_err(|err| err.to_string()),
			platform.sent_rpc.lock().expect("rpc list mutex poisoned").len(),
		),
		(Err(AuthorityError::Disconnected.to_string()), 0),
	);
}
