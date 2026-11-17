//! An offload's recycling phase as payments run it.

use super::*;
use crate::coinage::{layer::Op, start::RecordParams, testing::started_layer};

fn op() -> Op {
	Op { key: key(), retry: Retry::ExpiryOnly }
}

/// The inputs and call of each record of the offload.
fn records(wallet: &TestWallet) -> Vec<(Vec<Asset>, Option<Call>)> {
	wallet
		.view()
		.registrations()
		.of_operation(&key())
		.map(|facts| {
			let params = facts.state.plan.as_ref().and_then(|plan| RecordParams::from_params(&plan.params));
			(facts.rows.inputs.clone(), params.map(|params| params.call))
		})
		.collect()
}

/// An offload's phases register under the offload's own key, so its
/// recycling is followed, cancelled and judged with it.
#[tokio::test]
async fn an_offload_recycles_the_coins_it_needs_under_its_own_key() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let (layer, ..) = started_layer(wallet.db.clone()).await;
	let coin = wallet.coin(4);
	layer.offload(op(), PurseId::MAIN, 9, [7; 32], Initiator::User, false).await.unwrap();
	assert_eq!(records(&wallet), [(vec![coin], Some(Call::Recycle))]);
}

/// The next phase waits until the last one is stable: a retried run does
/// not recycle again while the first recycling is in flight.
#[tokio::test]
async fn an_offload_whose_phase_is_in_flight_is_not_advanced_again() {
	let wallet = TestWallet::new(PrivacyPolicy::fastest());
	let (layer, ..) = started_layer(wallet.db.clone()).await;
	wallet.coin(4);
	wallet.coin(4);
	layer.offload(op(), PurseId::MAIN, 9, [7; 32], Initiator::User, false).await.unwrap();
	layer.run_due(NOW).await.unwrap();
	assert_eq!(records(&wallet).len(), 1);
}
