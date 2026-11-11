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

#[test]
fn partial_top_up_credit_survives_the_wire_version_change() {
	use parity_scale_codec::{Decode, Encode};

	let credited = (1_u128 << 96) + 42;
	let mut old_wire = vec![6];
	old_wire.extend_from_slice(&credited.to_le_bytes());
	let old = v01::HostPaymentTopUpError::decode(&mut old_wire.as_slice()).unwrap();
	let latest = HostPaymentTopUpError::V1(old).into_latest();
	assert_eq!(latest, v02::HostPaymentTopUpError::PartialPayment { credited });
	// The released v0.2 refusal keeps tag 5. Old tag 6 was a partial
	// payment, not v0.2's insufficient-funds extension.
	assert_eq!(v02::HostPaymentTopUpError::FundingNotServed.encode(), vec![5]);
	assert_eq!(latest.encode()[0], 7);
	assert_eq!(
		HostPaymentTopUpError::from_latest(latest, 1),
		HostPaymentTopUpError::V1(v01::HostPaymentTopUpError::PartialPayment { credited })
	);
}
