use super::*;
use crate::versioned::Versioned;

fn chain_unavailable() -> v02::HostCreateTransactionError {
	v02::HostCreateTransactionError::ChainUnavailable {
		reason: "cannot select a metadata block".to_string(),
	}
}

#[test]
fn chain_unavailable_reaches_a_v02_caller_as_itself() {
	// The variant exists so a product can retry on it alone.
	assert_eq!(
		HostCreateTransactionError::from_latest(chain_unavailable(), 2),
		HostCreateTransactionError::V2(chain_unavailable())
	);
}

#[test]
fn chain_unavailable_reaches_a_v01_caller_as_unknown() {
	// A v0.1 product cannot decode the new variant, so it gets `Unknown`
	// with the same reason.
	assert_eq!(
		HostCreateTransactionError::from_latest(chain_unavailable(), 1),
		HostCreateTransactionError::V1(v01::HostCreateTransactionError::Unknown {
			reason: "cannot select a metadata block".to_string(),
		})
	);
}

#[test]
fn a_v01_error_upgrades_without_becoming_chain_unavailable() {
	// v0.1 has no such variant, so an `Unknown` it reports stays `Unknown`.
	let reason = "cannot select a metadata block".to_string();
	assert_eq!(
		HostCreateTransactionError::V1(v01::HostCreateTransactionError::Unknown {
			reason: reason.clone()
		})
		.into_latest(),
		v02::HostCreateTransactionError::Unknown { reason }
	);
}

#[test]
fn create_transaction_moves_to_v02_as_a_whole() {
	// The client sends the newest request, so a request left on V1 would
	// hide the V2 error from every caller.
	assert_eq!(
		[
			ProductAccountTxPayload::LATEST,
			HostCreateTransactionResponse::LATEST,
			HostCreateTransactionError::LATEST,
		],
		[2; 3]
	);
}

#[test]
fn create_transaction_with_legacy_account_stays_on_v01() {
	// A V2 request would fail on every host that does not serve one, so a
	// product that calls only this method keeps reaching older hosts.
	assert_eq!(LegacyAccountTxPayload::LATEST, 1);
}

#[test]
fn v1_keeps_codec_index_zero_so_the_new_version_is_additive() {
	// Reordering the variants would renumber V1 and break every deployed
	// v0.1 peer.
	use parity_scale_codec::Encode;

	let v1 = HostCreateTransactionError::V1(v01::HostCreateTransactionError::Rejected);
	assert_eq!(v1.encode()[0], 0);
	assert_eq!(v1.version(), 1);
}
