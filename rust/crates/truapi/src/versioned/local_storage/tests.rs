use super::*;
use crate::versioned::Versioned;

#[test]
fn a_v01_request_upgrades_to_the_callers_own_storage() {
	// v0.1 had no way to say anything but "my own storage", so the upgrade
	// must not invent a target.
	let upgraded =
		HostLocalStorageReadRequest::V1(v01::HostLocalStorageReadRequest { key: "k".to_string() })
			.into_latest();
	assert_eq!(upgraded, v02::HostLocalStorageReadRequest { product: None, key: "k".to_string() });
}

#[test]
fn a_refusal_reaches_a_v02_peer_as_itself() {
	assert_eq!(
		HostLocalStorageReadError::from_latest(v02::HostLocalStorageReadError::AccessNotGranted, 2),
		HostLocalStorageReadError::V2(v02::HostLocalStorageReadError::AccessNotGranted)
	);
}

#[test]
fn a_refusal_downgrades_to_a_reason_a_v01_peer_can_read() {
	// Unreachable in practice — a v0.1 caller cannot address foreign
	// storage — but the downgrade has to be total.
	for downgraded in [
		HostLocalStorageReadError::from_latest(v02::HostLocalStorageReadError::AccessNotGranted, 1),
		HostLocalStorageReadError::from_latest(v02::HostLocalStorageReadError::AccessNotGranted, 0),
	] {
		assert_eq!(
			downgraded,
			HostLocalStorageReadError::V1(v01::HostLocalStorageReadError::Unknown {
				reason: v02::HostLocalStorageReadError::AccessNotGranted.to_string(),
			})
		);
	}
}

#[test]
fn a_v01_error_upgrades_without_becoming_a_refusal() {
	// v0.1 has no refusal variant, so nothing it reports may arrive as one.
	for (v1, expected) in [
		(v01::HostLocalStorageReadError::Full, v02::HostLocalStorageReadError::Full),
		(
			v01::HostLocalStorageReadError::Unknown { reason: "disk".to_string() },
			v02::HostLocalStorageReadError::Unknown { reason: "disk".to_string() },
		),
	] {
		assert_eq!(HostLocalStorageReadError::V1(v1).into_latest(), expected);
	}
}

#[test]
fn quota_exhaustion_keeps_its_own_variant_in_both_directions() {
	// `Full` is the one failure a v0.1 peer can act on differently from a
	// generic error, so it must not collapse into `Unknown`.
	assert_eq!(
		HostLocalStorageReadError::from_latest(v02::HostLocalStorageReadError::Full, 1),
		HostLocalStorageReadError::V1(v01::HostLocalStorageReadError::Full)
	);
	assert_eq!(
		HostLocalStorageReadError::from_latest(v02::HostLocalStorageReadError::Full, 2),
		HostLocalStorageReadError::V2(v02::HostLocalStorageReadError::Full)
	);
}

#[test]
fn v1_keeps_codec_index_zero_so_the_new_version_is_additive() {
	// Not the derive's behaviour under test but the wire's: reordering the
	// variants would renumber V1 and break every deployed v0.1 peer.
	use parity_scale_codec::Encode;

	let v1 =
		HostLocalStorageReadRequest::V1(v01::HostLocalStorageReadRequest { key: "k".to_string() });
	assert_eq!(v1.encode()[0], 0);
	assert_eq!(v1.version(), 1);
	assert_eq!(HostLocalStorageReadRequest::LATEST, 2);
}

#[test]
fn write_and_clear_stay_on_v01() {
	// The `storage` scope is read-only, so no caller can address another
	// product's storage for a write. A V2 on these would advertise reach the
	// manifest cannot grant.
	assert_eq!(HostLocalStorageWriteRequest::LATEST, 1);
	assert_eq!(HostLocalStorageClearRequest::LATEST, 1);
}
