use super::*;
use parity_scale_codec::{Decode, DecodeAll};
use verifiable::ring::{
	StaticChunk, ark_vrf::ring::SrsLookup, bandersnatch::BandersnatchSha512Ell2,
	ring_verifier_builder_params,
};

const DOMAIN: RingDomainSize = RingDomainSize::Domain11;

fn answer<T: Decode>(encoded: Vec<u8>) -> T {
	Result::<T, String>::decode_all(&mut &encoded[..])
		.expect("a SCALE-encoded answer")
		.expect("the operation succeeds")
}

fn key(seed: u8) -> [u8; 32] {
	answer(member(&[seed; 32]))
}

#[test]
fn member_and_alias_are_verifiable_s() {
	let secret = BandersnatchVrfVerifiable::new_secret([4; 32]);
	assert_eq!(key(4), BandersnatchVrfVerifiable::member_from_secret(&secret));
	assert_eq!(
		answer::<[u8; 32]>(alias(&[4; 32], b"context")),
		BandersnatchVrfVerifiable::alias_in_context(&secret, b"context").unwrap(),
	);
}

#[test]
fn a_signature_verifies() {
	let signature: Vec<u8> = answer(sign(&[4; 32], b"message"));
	assert!(BandersnatchVrfVerifiable::verify_signature(
		&signature.try_into().expect("a 64-byte signature"),
		b"message",
		&key(4),
	));
}

#[test]
fn a_proof_verifies_against_the_ring_commitment() {
	let ring = [key(1), key(2), key(3)];
	let (proof, alias): (Vec<u8>, [u8; 32]) =
		answer(prove(&[2; 32], DOMAIN.value(), &ring[1], &ring.concat(), b"context", b"message"));

	let builder = ring_verifier_builder_params::<BandersnatchSha512Ell2>(DOMAIN);
	let lookup = |range: core::ops::Range<usize>| {
		(&builder)
			.lookup(range)
			.map(|chunks| chunks.into_iter().map(StaticChunk).collect())
			.ok_or(())
	};
	let mut members = BandersnatchVrfVerifiable::start_members(DOMAIN);
	BandersnatchVrfVerifiable::push_members(&mut members, ring.iter().copied(), lookup)
		.expect("the ring fits the domain");

	assert_eq!(
		BandersnatchVrfVerifiable::validate(
			DOMAIN,
			&proof.try_into().expect("the proof fits its bound"),
			&BandersnatchVrfVerifiable::finish_members(members),
			b"context",
			b"message",
		),
		Ok(alias),
	);
}
