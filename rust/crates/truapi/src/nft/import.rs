//! Imports: moves of NFTs from keys the host did not derive, such as the keys
//! of a deposited cheque, into fresh keys of one purse.

use rusqlite::Transaction;
use truapi::latest::GenericError;

use super::{
	keys::{ImportedItemKey, ItemKey},
	operation::{
		DestinationKeys, OperationKey, StartOperationError, allocate_destinations, nft_domain,
	},
	purse::PurseId,
	scarcity::{TransferParams, TransferSigner},
};
use crate::{
	durable::{PolicyParams, RegistrationError, group_exists},
	unix_time::current_unix_secs,
};

/// How long an import waits for its key to hold an NFT. Longer than other
/// transfers wait, because the sender's NFT may still be on its way.
const IMPORT_WAIT_LIMIT_SECS: u64 = 60 * 60;

/// Items to import into `into` under `operation`, one per key.
pub struct ImportRequest {
	/// The operation the imports form.
	pub operation: OperationKey,
	/// The purse the items go into.
	pub into: PurseId,
	/// The keys the items are moved from, with their secrets.
	pub keys: Vec<ImportedItemKey>,
}

/// Why an import did not start. Secrets may already be stored; they stay
/// unused.
#[derive(Debug, thiserror::Error)]
pub enum StartImportError {
	/// The secrets could not be stored.
	#[error("storing imported secrets failed: {}", .0.reason)]
	StoreSecrets(GenericError),
	/// The moves were not registered.
	#[error(transparent)]
	Registration(#[from] RegistrationError<StartOperationError>),
}

/// What the registration write needs to plan an import: its keys without
/// their secrets.
pub struct ImportPlan {
	/// The operation the imports form.
	pub operation: OperationKey,
	/// The purse the items go into.
	pub into: PurseId,
	/// The keys the items are moved from.
	pub sources: Vec<ItemKey>,
}

impl ImportRequest {
	/// The plan of this import, without its secrets.
	pub fn plan(&self) -> ImportPlan {
		let sources = self.keys.iter().map(ImportedItemKey::key).collect();
		ImportPlan { operation: self.operation.clone(), into: self.into, sources }
	}
}

/// Works out, inside the registration write, the params of each move of
/// `plan`, or refuses: the operation exists, or destination keys cannot be
/// allocated in its purse. A move names no instance until its source is seen
/// holding one.
pub fn plan_imports(
	tx: &Transaction<'_>,
	plan: &ImportPlan,
	keys: DestinationKeys<'_>,
) -> Result<Vec<PolicyParams>, StartOperationError> {
	if group_exists(tx, &nft_domain(), plan.operation.group())? {
		return Err(StartOperationError::OperationExists);
	}
	let destinations = allocate_destinations(tx, plan.into, plan.sources.len(), keys)?;
	let give_up_after = current_unix_secs() + IMPORT_WAIT_LIMIT_SECS;
	let moves = plan.sources.iter().zip(destinations);
	Ok(moves
		.map(|(source, destination)| import_params(*source, destination, give_up_after))
		.collect())
}

/// The params of moving the NFT `source` will hold to `destination`, signed
/// with the imported secret of `source` and given up at `give_up_after`.
fn import_params(source: ItemKey, destination: ItemKey, give_up_after: u64) -> PolicyParams {
	TransferParams {
		source,
		signer: TransferSigner::Imported,
		destination,
		submitted_attempt: None,
		give_up_after,
	}
	.into()
}

#[cfg(test)]
mod tests {
	use std::{collections::BTreeSet, sync::Arc};

	use futures::{StreamExt, executor::block_on};
	use tempfile::TempDir;

	use super::*;
	use crate::{
		encrypted_storage::MemoryEncryptedStorage,
		nft::{
			AllocateItemsError, ImportedSecretStore, NftLedger, NftLedgerDeps, PurseTag,
			UnknownPurse,
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
			assert_eq!(
				stored.map(|secret| *secret.as_bytes()),
				Some(*imported.secret().as_bytes())
			);
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
		let expected_sources: Vec<_> = (0..2).map(|index| imported_test_key(index).key()).collect();
		assert_eq!(sources, expected_sources);
		assert_eq!(destinations.len(), 2);
		for params in &params {
			let expected = TransferParams {
				signer: TransferSigner::Imported,
				submitted_attempt: None,
				..*params
			};
			assert_eq!(*params, expected);
			let owner = block_on(importer.ledger.purse_of(params.destination)).unwrap();
			assert_eq!(owner, Some(INTO));
			let limit = params.give_up_after - before;
			assert!(
				(IMPORT_WAIT_LIMIT_SECS..=IMPORT_WAIT_LIMIT_SECS + 5).contains(&limit),
				"{limit}"
			);
		}
	}

	/// A deposit retried under the same key must not move the items twice.
	#[test]
	fn a_repeated_import_is_refused_and_registers_nothing_new() {
		let importer = importer();
		block_on(importer.ledger.start_imports(import_request(operation(1), INTO, 1))).unwrap();

		let repeated =
			block_on(importer.ledger.start_imports(import_request(operation(1), INTO, 1)));

		assert!(matches!(
			repeated,
			Err(StartImportError::Registration(RegistrationError::Hook(
				StartOperationError::OperationExists
			)))
		));
		let moves = block_on(importer.ledger.observe_operation(&operation(1)).next()).unwrap();
		assert_eq!(moves.unwrap().len(), 1);
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
				StartOperationError::Allocate(AllocateItemsError::UnknownPurse(UnknownPurse(
					PurseId(7)
				)))
			)))
		));
		assert_eq!(stored_params(&importer), vec![]);
	}
}
