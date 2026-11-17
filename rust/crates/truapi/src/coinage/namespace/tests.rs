use super::*;

const PHRASE: &str = "bottom drive obey lake curtain smoke basket hold race lonely fit walk";

fn entropy() -> Vec<u8> {
	bip39::Mnemonic::parse(PHRASE).expect("a BIP-39 phrase").to_entropy()
}

fn hex(bytes: &[u8]) -> String {
	bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn the_datastore_account_matches_the_shipped_wallet() {
	let (_, account) = datastore_key(&entropy()).unwrap();
	assert_eq!(hex(&account), "82fde43d07766255566803c486f9427cfb7026328a3d87cddd34b362acbdb54d");
}

#[test]
fn a_coin_account_matches_the_shipped_wallet() {
	let (_, account) = coin_key(&entropy(), PurseId::MAIN, InstallationId([0x5a; 32]), 7).unwrap();
	assert_eq!(hex(&account), "4eced11d18ac64e54559cc45eb79415ca66d9354e74394dc8729b69a0fbd9107");
}

/// The vector the shipped wallets are to pin as well, so a recovered seed
/// finds the entries another platform loaded.
#[test]
fn an_entry_seed_is_pinned_for_the_shipped_wallets() {
	let seed = entry_seed(&entropy(), PurseId::MAIN, InstallationId([0x5a; 32]), 7);
	assert_eq!(hex(&seed.0), "7c9e2011ed19b98e95bd8d2bfcbbfb9cedca4617061d2388b58cf68ff0b96754");
}

#[test]
fn chain_codes_follow_the_junction_values() {
	let mut coinage = vec![0x1c, 0x63, 0x6f, 0x69, 0x6e, 0x61, 0x67, 0x65];
	coinage.resize(32, 0);
	assert_eq!(Junction::Name(COINS).chain_code().to_vec(), coinage);
	let mut main = vec![0xff, 0xff, 0xff, 0xff];
	main.resize(32, 0);
	assert_eq!(Junction::Number(u64::from(PurseId::MAIN.0)).chain_code().to_vec(), main);
	assert_eq!(Junction::Installation(InstallationId([7; 32])).chain_code(), [7; 32]);
}

#[test]
fn a_coin_key_has_the_same_scalar_and_account_on_every_derivation() {
	let entropy = entropy();
	let installation = InstallationId([1; 32]);
	let (first, first_account) = coin_key(&entropy, PurseId(3), installation, 9).unwrap();
	let (second, second_account) = coin_key(&entropy, PurseId(3), installation, 9).unwrap();
	assert_eq!(first_account, second_account);
	assert_eq!(first.0[..32], second.0[..32]);
	let secret = schnorrkel::SecretKey::from_bytes(&first.0).unwrap();
	assert_eq!(secret.to_public().to_bytes(), first_account);
}

#[test]
fn an_entry_seed_is_the_entropy_hashed_once_per_junction() {
	let entropy = entropy();
	let installation = InstallationId([2; 32]);
	let mut expected = entropy.clone();
	for code in [
		Junction::Name(ENTRIES).chain_code(),
		Junction::Number(5).chain_code(),
		installation.0,
		Junction::Number(11).chain_code(),
	] {
		expected = blake2b_simd::Params::new().hash_length(32).key(&code).hash(&expected).as_bytes().to_vec();
	}
	assert_eq!(entry_seed(&entropy, PurseId(5), installation, 11).0.to_vec(), expected);
}

#[test]
fn entropy_of_the_wrong_length_is_refused() {
	assert_eq!(datastore_key(&[0; 15]).map(|(_, account)| account), Err(InvalidEntropy));
}

#[test]
fn secrets_never_show_in_debug_output() {
	let (secret, _) = datastore_key(&entropy()).unwrap();
	assert_eq!(format!("{secret:?}"), "CoinSecret(..)");
	let seed = entry_seed(&entropy(), PurseId::MAIN, InstallationId([0; 32]), 0);
	assert_eq!(format!("{seed:?}"), "EntrySeed(..)");
}

const INSTALLATION: InstallationId = InstallationId([4; 32]);

fn bytes(hex: &str) -> Vec<u8> {
	::hex::decode(hex).unwrap()
}

fn registry_key(entropy: &[u8]) -> RegistryKey {
	RegistryKey::of(&datastore_key(entropy).unwrap().0).unwrap()
}

/// The shipped wallets' vectors (iOS `InstallationRecordCipherTests`,
/// Android `InstallationRecordCipherTest`): the secret is the bytes 0 to 63,
/// the installation `[0x5a; 32]`.
#[test]
fn a_record_seals_and_opens_as_the_shipped_wallets_do() {
	let secret: [u8; 64] = core::array::from_fn(|byte| byte as u8);
	let key = RegistryKey::from_ed25519_secret(&secret);
	assert_eq!(key.0.to_vec(), bytes("fc7bd73006ab990f9649eebec3aab66023af1f064d33a694d4d7a4f3bef86748"));
	let installation = InstallationId([0x5a; 32]);
	let record = bytes(
		"2cb9ac950a3a69cfa00480e7e221d461069884d050d21b2ba1c05e0e9bd08b3fe0edf24784976d3bd6e5652bb755ca90d48906c10e655eb8f3acc4c6",
	);
	assert_eq!(key.seal(installation).to_vec(), record);
	let with_another_nonce = bytes(
		"000102030405060708090a0bcdc58e6066449725c690c637add5e75d5fae9ea0e7c5df58e99ac4e0923ba664ba2d2aa75adc69e254a3e31eb2df5595",
	);
	assert_eq!(key.open(&with_another_nonce), Some(installation), "a record sealed with a random nonce opens");
	assert_eq!(key.open(&record[..59]), None);
}

/// The key of the `bottom drive obey` phrase, as polkadot-js, Android and
/// iOS compute it.
#[test]
fn a_seeds_registry_key_matches_the_shipped_wallets() {
	let (datastore, account) = datastore_key(&entropy()).unwrap();
	assert_eq!(account.to_vec(), bytes("82fde43d07766255566803c486f9427cfb7026328a3d87cddd34b362acbdb54d"));
	let expanded = schnorrkel::SecretKey::from_bytes(&datastore.0).unwrap().to_ed25519_bytes();
	let expected_secret = concat!(
		"f88810d0da504a86f719c0c85cac296eb9d5d0e3e51d54b53cf15d96f6084849",
		"2dddcd798787957c4b5d3b960b4b78d0c925adeb662079c192c01ba13c4f9abe",
	);
	assert_eq!(expanded.to_vec(), bytes(expected_secret));
	let key = RegistryKey::of(&datastore).unwrap();
	assert_eq!(key.0.to_vec(), bytes("a217d0089ea2bc908772a8df8ab8f4aeb122eb1f41bbd26b6aff9c9386143f2a"));
}

#[test]
fn another_seeds_key_does_not_open_a_record() {
	let sealed = registry_key(&entropy()).seal(INSTALLATION);
	assert_eq!(sealed, registry_key(&entropy()).seal(INSTALLATION), "sealing is deterministic");
	assert_eq!(registry_key(&entropy()).open(&sealed), Some(INSTALLATION));
	let other = registry_key(&[8; 16]);
	assert_eq!(other.open(&sealed), None);
}
