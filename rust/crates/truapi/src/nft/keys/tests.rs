use subxt::tx::Signer;

use super::*;

#[test]
fn the_signer_at_a_slot_signs_for_the_key_derived_at_it() {
	let entropy = [0x42; 16];
	let slot = ItemSlot {
		purse: PurseId(3),
		installation: InstallationId([0x1d; 32]),
		index: ItemIndex(7),
	};

	let signer = item_signer_from_entropy(&entropy, slot).unwrap();
	let keys = item_keys_from_entropy(&entropy, &[slot]).unwrap();

	assert_eq!(signer.account_id().0, keys[0].0);
}

/// A cheque's receiver rebuilds each item key from its exported secret, so
/// the export must carry the full expanded secret of the right key.
#[test]
fn an_exported_secret_rebuilds_the_key_derived_at_its_slot_and_signs_for_it() {
	let entropy = [0x42; 16];
	let slot = ItemSlot {
		purse: PurseId(3),
		installation: InstallationId([0x1d; 32]),
		index: ItemIndex(7),
	};

	let secrets = item_secrets_from_entropy(&entropy, &[slot]).unwrap();
	let keys = item_keys_from_entropy(&entropy, &[slot]).unwrap();
	let imported = schnorrkel::SecretKey::from_bytes(secrets[0].as_bytes()).unwrap();
	let public = imported.to_public();
	let signature = imported.sign_simple(b"substrate", b"move", &public);

	assert_eq!(public.to_bytes(), keys[0].0);
	assert!(public.verify_simple(b"substrate", b"move", &signature).is_ok());
}
