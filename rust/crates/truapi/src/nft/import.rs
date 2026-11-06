//! Imports: moves of NFTs from keys the host did not derive, such as the keys
//! of a deposited cheque, into fresh keys of one purse.

use std::collections::BTreeSet;

use rusqlite::{Connection, Transaction};
use truapi::latest::GenericError;

use super::{
	keys::{ImportedItemKey, ItemKey},
	operation::{
		DestinationKeys, OperationKey, StartOperationError, allocate_destinations,
		validate_not_moving, validate_operation_unused,
	},
	purse::PurseId,
	scarcity::{TransferParams, TransferSigner},
};
use crate::{
	durable::{PolicyParams, RegistrationError},
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
#[derive(Clone)]
pub struct ImportPlan {
	/// The operation the imports form.
	pub operation: OperationKey,
	/// The purse the items go into.
	pub into: PurseId,
	/// The keys the items are moved from.
	pub sources: BTreeSet<ItemKey>,
}

impl ImportRequest {
	/// The plan of this import, without its secrets.
	pub fn plan(&self) -> ImportPlan {
		let sources = self.keys.iter().map(ImportedItemKey::key).collect();
		ImportPlan { operation: self.operation.clone(), into: self.into, sources }
	}
}

/// Refuses `plan` when its operation is already registered or a live
/// transaction already moves one of its keys, such as another deposit of the
/// same cheque.
pub fn validate_imports_can_start(
	conn: &Connection,
	plan: &ImportPlan,
) -> Result<(), StartOperationError> {
	validate_operation_unused(conn, &plan.operation)?;
	validate_not_moving(conn, &plan.sources)
}

/// Works out, inside the registration write, the params of each move of
/// `plan`, in the order of its keys, or refuses: the plan cannot start, or
/// destination keys cannot be allocated in its purse. A move names no
/// instance until its source is seen holding one.
pub fn plan_imports(
	tx: &Transaction<'_>,
	plan: &ImportPlan,
	keys: DestinationKeys<'_>,
) -> Result<Vec<PolicyParams>, StartOperationError> {
	validate_imports_can_start(tx, plan)?;
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
mod tests;
