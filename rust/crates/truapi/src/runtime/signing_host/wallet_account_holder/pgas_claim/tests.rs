use super::*;
use crate::{
	runtime::signing_host::wallet_account_holder::tests::unlocked_wallet,
	test_support::StubPlatform,
};

fn refused_claim(wallet: &WalletAccountHolder) -> ClaimBuildError {
	let excluded = BTreeSet::new();
	let build = wallet.build_pgas_claim(&[0x44; 32], &excluded);
	futures::executor::block_on(build).err().expect("the claim is not built").into()
}

#[test]
fn a_claim_with_nobody_signed_in_waits_for_a_session() {
	let wallet = unlocked_wallet(StubPlatform::default(), "dot", 7);
	wallet.clear();

	let refused = refused_claim(&wallet);

	assert!(matches!(refused, ClaimBuildError::NoSession), "{refused}");
}

/// The signed-in session reaches the chain: here, an Asset Hub the stub
/// cannot connect to, which the claim waits out.
#[test]
fn a_claim_of_the_signed_in_session_is_built_from_the_chain() {
	let platform = StubPlatform { chain_connect_error: Some("no Asset Hub"), ..Default::default() };
	let wallet = unlocked_wallet(platform, "dot", 7);

	let refused = refused_claim(&wallet);

	assert!(matches!(refused, ClaimBuildError::Transient(_)), "{refused}");
}

#[test]
fn a_person_in_no_ring_cannot_build_a_claim() {
	let missing = AllowanceAllocationError::MissingPersonhoodMembership { resource: "PGAS" };

	assert!(matches!(ClaimBuildError::from(missing), ClaimBuildError::NoPersonhood));
}

#[test]
fn a_day_without_a_free_slot_cannot_build_a_claim() {
	let full = StatementAllowanceError::from(SlotError::NoFreePgasSlot { day: 9, max: 4 });

	let mapped = ClaimBuildError::from(AllowanceAllocationError::from(full));

	assert!(matches!(mapped, ClaimBuildError::NoFreeSlot { day: 9, max: 4 }), "{mapped}");
}

/// A session that ends while the claim is built waits for the next one.
#[test]
fn a_session_lost_while_building_waits_for_a_session() {
	let lost = StatementAllowanceError::from(AuthorityError::Disconnected);

	let mapped = ClaimBuildError::from(AllowanceAllocationError::from(lost));

	assert!(matches!(mapped, ClaimBuildError::NoSession), "{mapped}");
}
