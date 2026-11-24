use core::num::NonZeroU32;

use futures::executor::block_on;
use tempfile::TempDir;

use super::*;
use crate::{
	durable::DurableTxId,
	nft::{
		NftLedger, NftLedgerDeps, OperationKey, PurseId, PurseTag, TransferRequest, TransferType,
		testing::{TEST_MAXIMUM_MOVES, holding_after_moves, stored_transfer_params, test_ledger_deps},
	},
	store::{Db, open_temporary_core_db},
};

const FROM: PurseId = PurseId(0);
const TO: PurseId = PurseId(1);

/// A ledger with two purses and the claims it registers through, with one
/// key of [`FROM`] per entry of `moves`, holding an NFT that used that many
/// fee-less moves.
struct Purses {
	_dir: TempDir,
	db: Db,
	ledger: NftLedger,
	claims: PgasClaims,
	held: Vec<ItemKey>,
}

fn purses_holding_after(moves: &[u16]) -> Purses {
	let (dir, db) = open_temporary_core_db();
	let deps: NftLedgerDeps = test_ledger_deps(db.clone());
	let claims = deps.claims.clone();
	let ledger = NftLedger::new(deps);
	for name in ["from", "to"] {
		block_on(ledger.create_purse(name.to_owned(), PurseTag::default())).unwrap();
	}
	let count = NonZeroU32::new(u32::try_from(moves.len()).unwrap()).unwrap();
	let held = block_on(ledger.allocate_items(FROM, count)).unwrap();
	let holdings = held
		.iter()
		.zip(moves)
		.enumerate()
		.map(|(instance, (key, moves))| (*key, Some(holding_after_moves(instance as u64, *moves))))
		.collect();
	block_on(ledger.record_holdings(holdings)).unwrap();
	Purses { _dir: dir, db, ledger, claims, held }
}

fn rebalance(purses: &Purses, items: &[ItemKey]) -> Result<(), RegistrationError<StartOperationError>> {
	let inputs = items.iter().copied().collect();
	let operation = OperationKey::new(b"product", [1; 32]);
	let request = TransferRequest { operation, from: FROM, transfer: TransferType::Local { inputs, into: TO } };
	block_on(purses.ledger.start_transfers(request))
}

/// One registered NFT move: the key it spends, its mode, and the
/// transactions it waits for.
#[derive(Debug, PartialEq, Eq)]
struct RegisteredMove {
	source: ItemKey,
	mode: TransferMode,
	waits_for: Vec<DurableTxId>,
}

/// Every registered NFT move, in registration order, which is key order.
fn registered_moves(db: &Db) -> Vec<RegisteredMove> {
	let params = stored_transfer_params(db);
	let read = |conn: &rusqlite::Connection| {
		let mut statement = conn.prepare(
			"SELECT tx.id, dependency.depends_on FROM durable_tx AS tx
			LEFT JOIN durable_tx_dependency AS dependency ON dependency.tx_id = tx.id
			WHERE tx.domain = 'nft' ORDER BY tx.id",
		)?;
		let rows = statement.query_map([], |row| row.get::<_, Option<i64>>(1))?;
		Ok(rows.collect::<Result<Vec<_>, _>>()?)
	};
	let waits = block_on(db.read(read)).unwrap();
	params
		.into_iter()
		.zip(waits)
		.map(|(params, waits_for)| RegisteredMove {
			source: params.source,
			mode: params.mode,
			waits_for: waits_for.map(DurableTxId).into_iter().collect(),
		})
		.collect()
}

/// The ids of every registered PGAS claim, in registration order.
fn registered_claims(db: &Db) -> Vec<DurableTxId> {
	let read = |conn: &rusqlite::Connection| {
		let mut statement = conn.prepare("SELECT id FROM durable_tx WHERE domain = 'pgas' ORDER BY id")?;
		let rows = statement.query_map([], |row| row.get::<_, i64>(0))?;
		Ok(rows.collect::<Result<Vec<_>, _>>()?)
	};
	block_on(db.read(read)).unwrap().into_iter().map(DurableTxId).collect()
}

/// An exhausted NFT can only move paid, from PGAS claimed into its key, so
/// its move waits for that claim. An NFT with a fee-less move left needs
/// neither.
#[test]
fn an_exhausted_item_moves_paid_after_a_claim_into_its_key() {
	let purses = purses_holding_after(&[0, TEST_MAXIMUM_MOVES]);

	rebalance(&purses, &purses.held).unwrap();

	let claims = registered_claims(&purses.db);
	let mut expected = vec![
		RegisteredMove { source: purses.held[0], mode: TransferMode::FeeLess, waits_for: vec![] },
		RegisteredMove { source: purses.held[1], mode: TransferMode::Paid, waits_for: claims.clone() },
	];
	expected.sort_by_key(|registered| registered.source);
	assert_eq!(claims.len(), 1);
	assert_eq!(registered_moves(&purses.db), expected);
}

/// A key has at most one active claim, so a move whose key is already being
/// claimed into waits for that claim instead of spending another slot.
#[test]
fn a_paid_move_waits_for_the_claim_already_active_in_its_key() {
	let purses = purses_holding_after(&[TEST_MAXIMUM_MOVES]);
	let active = block_on(purses.claims.claim(AccountId32(purses.held[0].0))).unwrap();

	rebalance(&purses, &purses.held).unwrap();

	assert_eq!(registered_claims(&purses.db), vec![active]);
	assert_eq!(registered_moves(&purses.db)[0].waits_for, vec![active]);
}

/// Until the tracker recorded an NFT's moves, the mode cannot be known, and
/// guessing fee-less could fail a move that had to be paid.
#[test]
fn an_item_whose_moves_are_not_recorded_yet_is_refused_as_unavailable() {
	let purses = purses_holding_after(&[0]);
	let forget = |tx: &rusqlite::Transaction<'_>| {
		tx.execute("UPDATE nft_holding SET moves = NULL", [])?;
		Ok(())
	};
	block_on(purses.db.write(forget)).unwrap();

	let refused = rebalance(&purses, &purses.held);

	assert!(matches!(
		refused,
		Err(RegistrationError::Hook(StartOperationError::ItemUnavailable(key))) if key == purses.held[0]
	));
	assert_eq!(registered_moves(&purses.db), vec![]);
}
