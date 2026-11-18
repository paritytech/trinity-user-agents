use super::*;

fn bytes(hex: &str) -> [u8; 32] {
	hex::decode(hex).unwrap().try_into().unwrap()
}

#[test]
fn a_receivable_is_the_rfc_7748_public_key_of_its_draw() {
	let alice = bytes("77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a");
	let (receivable, secret) = new_receivable(alice);
	assert_eq!(receivable, Receivable(bytes("8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a")));
	assert_eq!(*secret.0, StaticSecret::from(alice).to_bytes(), "the secret is the draw, clamped");
}

#[test]
fn a_secret_never_shows_in_debug_output() {
	let (_, secret) = new_receivable([5; 32]);
	assert_eq!(format!("{secret:?}"), "ReceivableSecret(..)");
}
