use subxt::tx::Signer;

use super::*;

#[test]
fn the_signer_at_a_slot_signs_for_the_key_derived_at_it() {
	let entropy = RootEntropy::new([0x42; 16]);
	let slot = ItemSlot {
		purse: PurseId(3),
		installation: InstallationId([0x1d; 32]),
		index: ItemIndex(7),
	};

	let signer = entropy.item_signer(slot).unwrap();
	let keys = entropy.item_keys(&[slot]).unwrap();

	assert_eq!(signer.account_id().0, keys[0].0);
}

/// A cheque's receiver rebuilds each item key from its exported secret, so
/// the export must carry the full expanded secret of the right key.
#[test]
fn an_exported_secret_rebuilds_the_key_derived_at_its_slot_and_signs_for_it() {
	let entropy = RootEntropy::new([0x42; 16]);
	let slot = ItemSlot {
		purse: PurseId(3),
		installation: InstallationId([0x1d; 32]),
		index: ItemIndex(7),
	};

	let secrets = entropy.item_secrets(&[slot]).unwrap();
	let keys = entropy.item_keys(&[slot]).unwrap();
	let imported = schnorrkel::SecretKey::from_bytes(secrets[0].as_bytes()).unwrap();
	let public = imported.to_public();
	let signature = imported.sign_simple(b"substrate", b"move", &public);

	assert_eq!(public.to_bytes(), keys[0].0);
	assert!(public.verify_simple(b"substrate", b"move", &signature).is_ok());
}

/// Recovery derives item keys a range at a time under the item key prefix,
/// so it must find exactly the keys allocation derived one slot at a time.
#[test]
fn keys_under_the_item_prefix_are_the_allocated_item_keys() {
	let entropy = RootEntropy::new([0x42; 16]);
	let (purse, installation) = (PurseId(3), InstallationId([0x1d; 32]));
	let slots = [0, 1, 7].map(|index| ItemSlot { purse, installation, index: ItemIndex(index) });

	let range = entropy.item_key_range(purse, installation, 0..8).unwrap();
	let allocated = entropy.item_keys(&slots).unwrap();

	assert_eq!([0, 1, 7].map(|index| range[index]), allocated.as_slice());
}
