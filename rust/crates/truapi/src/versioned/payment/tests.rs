use super::*;

#[test]
fn a_v01_top_up_upgrades_without_a_funding_session() {
	let source = v01::PaymentTopUpSource::ProductAccount {
		derivation_index: v01::DerivationIndex::Index(0),
	};
	let upgraded = HostPaymentTopUpRequest::V1(v01::HostPaymentTopUpRequest {
		into: None,
		amount: 5,
		source: source.clone(),
		id: [7; 32],
	})
	.into_latest();
	assert_eq!(
		upgraded,
		v02::HostPaymentTopUpRequest { into: None, amount: 5, source, id: [7; 32], funding: None }
	);
}

#[test]
fn a_v01_payment_upgrades_without_a_funding_session() {
	let upgraded = HostPaymentRequest::V1(v01::HostPaymentRequest {
		from: None,
		amount: 5,
		destination: [1; 32],
		id: [7; 32],
	})
	.into_latest();
	assert_eq!(
		upgraded,
		v02::HostPaymentRequest {
			from: None,
			amount: 5,
			destination: [1; 32],
			id: [7; 32],
			funding: None
		}
	);
}

// v0.1 has no variant for it, so it arrives as a catch-all carrying its text.
#[test]
fn a_funding_refusal_reaches_a_v01_peer_as_unknown() {
	assert_eq!(
		(
			HostPaymentTopUpError::from_latest(v02::HostPaymentTopUpError::FundingNotServed, 1),
			HostPaymentError::from_latest(v02::HostPaymentError::FundingNotServed, 1),
		),
		(
			HostPaymentTopUpError::V1(v01::HostPaymentTopUpError::Unknown {
				reason: "the funding session does not take this top-up".to_string(),
			}),
			HostPaymentError::V1(v01::HostPaymentError::Unknown {
				reason: "the funding session does not take this payment".to_string(),
			}),
		)
	);
}

#[test]
fn a_funding_refusal_reaches_a_v02_peer_as_itself() {
	assert_eq!(
		HostPaymentTopUpError::from_latest(v02::HostPaymentTopUpError::FundingNotServed, 2),
		HostPaymentTopUpError::V2(v02::HostPaymentTopUpError::FundingNotServed)
	);
}
