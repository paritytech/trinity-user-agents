use std::{collections::BTreeSet, sync::Arc};

use futures::{StreamExt, executor::block_on};
use tempfile::TempDir;

use super::*;
use crate::{
	encrypted_storage::MemoryEncryptedStorage,
	nft::{
		AllocateItemsError, ImportedSecretStore, NftLedger, NftLedgerDeps, PurseTag, UnknownPurse,
		testing::{imported_test_key, test_ledger_deps},
	},
	store::{Db, open_temporary_core_db},
};

const INTO: PurseId = PurseId(0);

/// A ledger with one purse, whose imported secrets a test can read back.
struct Importer {
	_dir: TempDir,
	db: Db,
	ledger: NftLedger,
	imported: ImportedSecretStore,
}

fn importer() -> Importer {
	let (dir, db) = open_temporary_core_db();
	let imported = ImportedSecretStore::new(Arc::new(MemoryEncryptedStorage::default()));
	let deps = NftLedgerDeps { imported: imported.clone(), ..test_ledger_deps(db.clone()) };
	let ledger = NftLedger::new(deps);
	block_on(ledger.create_purse("tickets".to_owned(), PurseTag::default())).unwrap();
	Importer { _dir: dir, db, ledger, imported }
}

fn operation(byte: u8) -> OperationKey {
	OperationKey::new(b"product", [byte; 32])
}

fn import_request(operation: OperationKey, into: PurseId, count: u32) -> ImportRequest {
	ImportRequest { operation, into, keys: (0..count).map(imported_test_key).collect() }
}

/// The params of every registered move, as the policy reads them back.
fn stored_params(importer: &Importer) -> Vec<TransferParams> {
	let stored = |conn: &rusqlite::Connection| {
		let mut statement = conn.prepare("SELECT policy_params FROM durable_tx ORDER BY id")?;
		let rows = statement.query_map([], |row| row.get::<_, Vec<u8>>(0))?;
		Ok(rows.collect::<Result<Vec<_>, _>>()?)
	};
	let rows = block_on(importer.db.read(stored)).unwrap();
	rows.into_iter()
		.map(|row| TransferParams::try_from(&PolicyParams(row)).unwrap())
		.collect()
}

/// Each move is signed from its stored secret, possibly after a restart,
/// so the secrets are stored before any move exists.
#[test]
fn an_import_stores_the_secret_of_every_key() {
	let importer = importer();

	block_on(importer.ledger.start_imports(import_request(operation(1), INTO, 2))).unwrap();

	for index in 0..2 {
		let imported = imported_test_key(index);
		let stored = block_on(importer.imported.secret_of(&imported.key())).unwrap();
		assert_eq!(stored.map(|secret| *secret.as_bytes()), Some(*imported.secret().as_bytes()));
	}
}

/// The host did not derive the cheque keys and does not know their
/// instances, so each move signs with the imported secret and waits for
/// any NFT, for longer than a move of a key the product already saw.
#[test]
fn an_import_registers_one_waiting_move_per_key_into_fresh_keys_of_the_purse() {
	let importer = importer();
	let before = current_unix_secs();

	block_on(importer.ledger.start_imports(import_request(operation(1), INTO, 2))).unwrap();

	let params = stored_params(&importer);
	let sources: Vec<_> = params.iter().map(|params| params.source).collect();
	let destinations: BTreeSet<_> = params.iter().map(|params| params.destination).collect();
	let expected_sources: Vec<_> = (0..2)
		.map(|index| imported_test_key(index).key())
		.collect::<BTreeSet<_>>()
		.into_iter()
		.collect();
	assert_eq!(sources, expected_sources);
	assert_eq!(destinations.len(), 2);
	for params in &params {
		let expected =
			TransferParams { signer: TransferSigner::Imported, submitted_attempt: None, ..*params };
		assert_eq!(*params, expected);
		let owner = block_on(importer.ledger.purse_of(params.destination)).unwrap();
		assert_eq!(owner, Some(INTO));
		let limit = params.give_up_after - before;
		assert!((IMPORT_WAIT_LIMIT_SECS..=IMPORT_WAIT_LIMIT_SECS + 5).contains(&limit), "{limit}");
	}
}

/// A deposit retried under the same key must not move the items twice.
#[test]
fn a_repeated_import_is_refused_and_registers_nothing_new() {
	let importer = importer();
	block_on(importer.ledger.start_imports(import_request(operation(1), INTO, 1))).unwrap();

	let repeated = block_on(importer.ledger.start_imports(import_request(operation(1), INTO, 1)));

	assert!(matches!(
		repeated,
		Err(StartImportError::Registration(RegistrationError::Hook(
			StartOperationError::OperationExists
		)))
	));
	let moves = block_on(importer.ledger.observe_operation(&operation(1)).next()).unwrap();
	assert_eq!(moves.unwrap().len(), 1);
}

/// Only one move of a key can land, so a key another import moves is
/// refused, before any secret of the refused import is stored.
#[test]
fn an_import_of_a_key_another_import_moves_is_refused_before_storing_secrets() {
	let importer = importer();
	block_on(importer.ledger.start_imports(import_request(operation(1), INTO, 1))).unwrap();

	let overlapping =
		block_on(importer.ledger.start_imports(import_request(operation(2), INTO, 2)));

	let moved = imported_test_key(0).key();
	assert!(matches!(
		overlapping,
		Err(StartImportError::Registration(RegistrationError::Hook(
			StartOperationError::ItemUnavailable(key)
		))) if key == moved
	));
	let unmoved = block_on(importer.imported.secret_of(&imported_test_key(1).key())).unwrap();
	assert!(unmoved.is_none());
}

/// Items go only into a purse that exists, and a refused import registers
/// nothing.
#[test]
fn an_import_into_an_unknown_purse_is_refused() {
	let importer = importer();

	let refused =
		block_on(importer.ledger.start_imports(import_request(operation(1), PurseId(7), 1)));

	assert!(matches!(
		refused,
		Err(StartImportError::Registration(RegistrationError::Hook(
			StartOperationError::Allocate(AllocateItemsError::UnknownPurse(UnknownPurse(PurseId(
				7
			))))
		)))
	));
	assert_eq!(stored_params(&importer), vec![]);
}
