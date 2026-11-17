use super::*;
use futures::executor::block_on;

#[test]
fn exponent_maps_to_domain() {
	assert_eq!(domain_for_ring_exponent(9).unwrap(), vrf::DOMAIN_2E11);
	assert_eq!(domain_for_ring_exponent(10).unwrap(), vrf::DOMAIN_2E12);
	assert_eq!(domain_for_ring_exponent(14).unwrap(), vrf::DOMAIN_2E16);
	assert!(domain_for_ring_exponent(11).is_err());
}

#[test]
fn proof_is_785_bytes_for_a_single_member_ring() {
	let entropy = [0x11u8; 32];
	let member = member_key_now(entropy);
	let members = vec![member];
	let proof = ring_vrf_proof(
		&block_on(vrf::load()).unwrap(),
		vrf::DOMAIN_2E11,
		&entropy,
		&members,
		&[0x33; 32],
		&[0x42; 32],
	)
	.unwrap();
	assert_eq!(proof.len(), RING_VRF_PROOF_LEN);
}

#[test]
fn open_fails_when_member_absent_from_ring() {
	let entropy = [0x11u8; 32];
	let other = member_key_now([0x22u8; 32]);
	let err = ring_vrf_proof(
		&block_on(vrf::load()).unwrap(),
		vrf::DOMAIN_2E11,
		&entropy,
		&[other],
		&[0x33; 32],
		&[0x42; 32],
	)
	.unwrap_err();
	assert!(err.to_string().contains("NotInRing"), "unexpected error: {err}");
}
