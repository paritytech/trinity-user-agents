use super::*;
use crate::{
	runtime::signing_host::wallet_account_holder::tests::unlocked_wallet,
	test_support::StubPlatform,
};

fn build_as(wallet: &WalletAccountHolder, identity: ClaimIdentity) -> ClaimBuildError {
	let excluded = BTreeSet::new();
	let build = wallet.build_pgas_claim_for_identity([0x44; 32], identity, &excluded);
	futures::executor::block_on(build).err().expect("the claim is not built")
}

fn signed_in_identity(wallet: &WalletAccountHolder) -> ClaimIdentity {
	ClaimIdentity::from_root_public_key(wallet.current_session().unwrap().public_key)
}

/// Signing out and in as someone else never spends their slot on the first
/// user's claim.
#[test]
fn a_claim_registered_by_another_identity_is_refused() {
	let wallet = unlocked_wallet(StubPlatform::default(), "dot", 7);
	let other = ClaimIdentity::from_root_public_key([0x2e; 32]);

	let refused = build_as(&wallet, other);

	assert!(matches!(refused, ClaimBuildError::OtherIdentity), "{refused}");
}

#[test]
fn a_claim_with_nobody_signed_in_waits_for_a_session() {
	let wallet = unlocked_wallet(StubPlatform::default(), "dot", 7);
	let identity = signed_in_identity(&wallet);
	wallet.clear();

	let refused = build_as(&wallet, identity);

	assert!(matches!(refused, ClaimBuildError::NoSession), "{refused}");
}

/// The signed-in identity reaches the chain: here, an Asset Hub the stub
/// cannot connect to, which the claim waits out.
#[test]
fn a_claim_of_the_signed_in_identity_is_built_from_the_chain() {
	let platform = StubPlatform { chain_connect_error: Some("no Asset Hub"), ..Default::default() };
	let wallet = unlocked_wallet(platform, "dot", 7);
	let identity = signed_in_identity(&wallet);

	let refused = build_as(&wallet, identity);

	assert!(matches!(refused, ClaimBuildError::Transient(_)), "{refused}");
}

#[test]
fn a_person_in_no_ring_cannot_build_a_claim() {
	let missing = AllowanceAllocationError::MissingPersonhoodMembership { resource: "PGAS" };

	assert!(matches!(claim_build_error(missing), ClaimBuildError::NoPersonhood));
}

#[test]
fn a_day_without_a_free_slot_cannot_build_a_claim() {
	let full = StatementAllowanceError::from(SlotError::NoFreePgasSlot { day: 9, max: 4 });

	let mapped = claim_build_error(full.into());

	assert!(matches!(mapped, ClaimBuildError::NoFreeSlot { day: 9, max: 4 }), "{mapped}");
}

/// A session that ends while the claim is built waits for the next one.
#[test]
fn a_session_lost_while_building_waits_for_a_session() {
	let lost = StatementAllowanceError::from(AuthorityError::Disconnected);

	let mapped = claim_build_error(lost.into());

	assert!(matches!(mapped, ClaimBuildError::NoSession), "{mapped}");
}
