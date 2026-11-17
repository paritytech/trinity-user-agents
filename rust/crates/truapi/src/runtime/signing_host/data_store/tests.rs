use super::*;

/// The secret the apps' record cipher tests pin their vectors to.
fn pinned_secret() -> [u8; 64] {
	core::array::from_fn(|index| u8::try_from(index).expect("the index is below 64"))
}

fn bytes(hex: &str) -> Vec<u8> {
	hex::decode(hex.trim_start_matches("0x")).expect("vector is hex")
}

/// The installation the apps' vectors seal.
const PINNED_INSTALLATION: InstallationId = InstallationId([0x5a; 32]);

/// Sealed by the apps under the pinned secret, with the nonce they derive.
const PINNED_RECORD: &str = "0x2cb9ac950a3a69cfa00480e7e221d461069884d050d21b2ba1c05e0e9bd08b3fe0edf24784976d3bd6e5652bb755ca90d48906c10e655eb8f3acc4c6";

/// The same installation sealed under the pinned secret with another nonce.
const PINNED_RECORD_WITH_ANOTHER_NONCE: &str = "0x000102030405060708090a0bcdc58e6066449725c690c637add5e75d5fae9ea0e7c5df58e99ac4e0923ba664ba2d2aa75adc69e254a3e31eb2df5595";

/// The core opens what the Android and iOS apps seal, whatever nonce they
/// chose, so it must derive the key they derive from the same secret. Anything
/// else in the seed's list is skipped rather than failing the read.
#[test]
fn only_records_sealed_under_the_seed_open_and_each_installation_comes_back_once() {
	let key = record_key_from_secret(&pinned_secret());
	let other_seed = record_key_from_secret(&[7; 64]);
	let mut tampered = bytes(PINNED_RECORD);
	tampered[20] ^= 1;
	let records = [
		bytes(PINNED_RECORD),
		tampered,
		other_seed.encrypt_with_nonce(&[0x11; 32], [0; 12]).unwrap(),
		vec![1, 2, 3],
		bytes(PINNED_RECORD_WITH_ANOTHER_NONCE),
	];

	assert_eq!(open_installation_records(&key, &records), [PINNED_INSTALLATION]);
}

/// The apps' own vectors, pinned against polkadot-js: the seed's `//datastore`
/// account and the key its records are sealed under. A drift would make the
/// core read another account's list, or open nothing, without any error.
#[test]
fn the_data_store_account_and_record_key_match_the_apps() {
	let mnemonic = "bottom drive obey lake curtain smoke basket hold race lonely fit walk";
	let entropy = bip39::Mnemonic::parse(mnemonic).unwrap().to_entropy();
	let public_key: [u8; 32] =
		bytes("0x82fde43d07766255566803c486f9427cfb7026328a3d87cddd34b362acbdb54d")
			.try_into()
			.unwrap();
	let app_key = AeadKey::from_bytes(Zeroizing::new(
		bytes("0xa217d0089ea2bc908772a8df8ab8f4aeb122eb1f41bbd26b6aff9c9386143f2a")
			.try_into()
			.unwrap(),
	));
	let sealed_by_app = app_key.encrypt_with_nonce(&PINNED_INSTALLATION.0, [9; 12]).unwrap();

	let owner = data_store_owner(&entropy).unwrap();
	let key = installation_record_key(&entropy).unwrap();

	assert_eq!(owner, account_to_h160(&public_key));
	assert_eq!(open_installation_records(&key, &[sealed_by_app]), [PINNED_INSTALLATION]);
}
