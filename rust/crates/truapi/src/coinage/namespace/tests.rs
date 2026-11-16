use std::collections::HashSet;

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
fn distinct_paths_give_distinct_keys() {
	let entropy = entropy();
	let vrf = futures::executor::block_on(crate::runtime::load_vrf()).unwrap();
	let mut accounts = HashSet::new();
	let mut members = HashSet::new();
	let mut seeds = HashSet::new();
	for purse in [PurseId(0), PurseId(1), PurseId::MAIN] {
		for installation in [InstallationId([0; 32]), InstallationId([1; 32])] {
			for item in 0..4 {
				let (_, account) = coin_key(&entropy, purse, installation, item).unwrap();
				assert!(accounts.insert(account), "coin {purse:?} {installation:?} {item}");
				let (seed, member) = entry_key(&entropy, purse, installation, item, &vrf).unwrap();
				assert!(seeds.insert(seed.0), "entry seed {purse:?} {installation:?} {item}");
				assert!(members.insert(member), "entry member {purse:?} {installation:?} {item}");
			}
		}
	}
	assert!(accounts.iter().all(|account| !members.contains(account)));
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
