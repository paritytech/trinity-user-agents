//! Queries over the `durable_tx` ledger, one SQL constant and one function
//! each.

use futures::stream::BoxStream;
use rusqlite::{
	Connection, Row, Transaction, named_params,
	types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, Type, ValueRef},
};
use subxt::utils::H256;

use super::model::{
	DomainId, DurableTxEntry, DurableTxId, DurableTxState, DurableTxStatus, FailureKind, GroupId,
	Verdict,
};
use crate::{
	chain::{HashAndNumber, MortalExtrinsic, Mortality},
	store::{Db, DbError},
};

const INSERT: &str = "INSERT INTO durable_tx
    (domain, group_id, tx_hash, birth_number, birth_hash, period, status)
    VALUES (:domain, :group_id, :tx_hash, :birth_number, :birth_hash, :period, 'PENDING')
    RETURNING id";

/// Records a presigned extrinsic as a `Pending` row.
pub fn insert(
	tx: &Transaction<'_>,
	domain: &DomainId,
	group: Option<&GroupId>,
	extrinsic: &MortalExtrinsic,
) -> Result<DurableTxId, DbError> {
	let birth = extrinsic.mortality.birth();
	let id = tx.prepare_cached(INSERT)?.query_row(
		named_params! {
			":domain": domain.as_str(),
			":group_id": group.map(GroupId::as_str),
			":tx_hash": extrinsic.extrinsic.hash().0,
			":birth_number": birth.number,
			":birth_hash": birth.hash.0,
			":period": extrinsic.mortality.period(),
		},
		|row| row.get("id").map(DurableTxId),
	)?;
	Ok(id)
}

const COMPARE_AND_SET: &str = "UPDATE durable_tx
    SET status = :status, success_number = :success_number, success_hash = :success_hash,
        failure_kind = :failure_kind, scanned_number = COALESCE(:scanned_number, scanned_number)
    WHERE id = :id AND status = :expected AND tx_hash = :expected_hash
    AND status IN ('PENDING', 'PENDING_SUCCESS')";

/// Writes `verdict` only while the row still awaits a verdict in the status
/// and attempt `observed` holds. Returns whether it wrote.
pub fn compare_and_set(
	tx: &Transaction<'_>,
	observed: &DurableTxEntry,
	verdict: &Verdict,
) -> Result<bool, DbError> {
	let success = verdict.status.success_block();
	let changed = tx.prepare_cached(COMPARE_AND_SET)?.execute(named_params! {
		":status": status_name(verdict.status),
		":success_number": success.map(|block| block.number),
		":success_hash": success.map(|block| block.hash.0),
		":failure_kind": failure_kind(verdict.status),
		":scanned_number": verdict.scanned_to,
		":id": observed.id.0,
		":expected": status_name(observed.status),
		":expected_hash": observed.tx_hash.0,
	})?;
	Ok(changed == 1)
}

const ENTRY: &str = "SELECT * FROM durable_tx WHERE id = :id";

/// The row with `id`.
pub fn entry(conn: &Connection, id: DurableTxId) -> Result<Option<DurableTxEntry>, DbError> {
	let mut stmt = conn.prepare_cached(ENTRY)?;
	let mut rows = stmt.query(named_params! { ":id": id.0 })?;
	Ok(rows.next()?.map(DurableTxEntry::from_row).transpose()?)
}

/// The columns [`status_from_row`] reads.
macro_rules! status_columns {
	() => {
		"status, success_number, success_hash, failure_kind"
	};
}

const STATUS: &str = concat!("SELECT ", status_columns!(), " FROM durable_tx WHERE id = :id");

/// The status of the row with `id`.
pub fn status(conn: &Connection, id: DurableTxId) -> Result<Option<DurableTxStatus>, DbError> {
	let mut stmt = conn.prepare_cached(STATUS)?;
	let mut rows = stmt.query(named_params! { ":id": id.0 })?;
	Ok(rows.next()?.map(status_from_row).transpose()?)
}

/// [`status`], re-read after every commit that changes it.
pub fn observe_status(
	db: &Db,
	id: DurableTxId,
) -> BoxStream<'static, Result<Option<DurableTxStatus>, DbError>> {
	db.observe(STATUS, move |stmt| {
		stmt.query_optional(named_params! { ":id": id.0 }, status_from_row)
	})
}

const GROUP: &str = concat!(
	"SELECT id, ",
	status_columns!(),
	" FROM durable_tx WHERE domain = :domain AND group_id = :group_id ORDER BY id"
);

/// The transactions of one group, in registration order.
pub fn group(
	conn: &Connection,
	domain: &DomainId,
	group: &GroupId,
) -> Result<Vec<DurableTxState>, DbError> {
	Ok(conn
		.prepare_cached(GROUP)?
		.query_map(
			named_params! { ":domain": domain.as_str(), ":group_id": group.as_str() },
			DurableTxState::from_row,
		)?
		.collect::<rusqlite::Result<_>>()?)
}

/// [`group`], re-read after every commit that changes it.
pub fn observe_group(
	db: &Db,
	domain: DomainId,
	group: GroupId,
) -> BoxStream<'static, Result<Vec<DurableTxState>, DbError>> {
	db.observe(GROUP, move |stmt| {
		stmt.query_map(
			named_params! { ":domain": domain.as_str(), ":group_id": group.as_str() },
			DurableTxState::from_row,
		)
	})
}

const DOMAIN_ENTRIES: &str = "SELECT * FROM durable_tx WHERE domain = :domain ORDER BY id";

/// Every row of one domain, in registration order.
pub fn domain_entries(
	conn: &Connection,
	domain: &DomainId,
) -> Result<Vec<DurableTxEntry>, DbError> {
	Ok(conn
		.prepare_cached(DOMAIN_ENTRIES)?
		.query_map(named_params! { ":domain": domain.as_str() }, DurableTxEntry::from_row)?
		.collect::<rusqlite::Result<_>>()?)
}

const LIVE_DOMAINS: &str = "SELECT DISTINCT domain FROM durable_tx
    WHERE status IN ('PENDING', 'PENDING_SUCCESS') ORDER BY domain";

/// Every domain with a transaction still awaiting a verdict.
pub fn live_domains(conn: &Connection) -> Result<Vec<DomainId>, DbError> {
	Ok(conn
		.prepare_cached(LIVE_DOMAINS)?
		.query_map([], |row| row.get::<_, String>("domain").map(DomainId::new))?
		.collect::<rusqlite::Result<_>>()?)
}

const HAS_LIVE: &str = "SELECT EXISTS (SELECT 1 FROM durable_tx
    WHERE status IN ('PENDING', 'PENDING_SUCCESS')) AS live";

/// Whether any transaction still awaits a verdict, re-read after every
/// commit that changes it.
pub fn observe_has_live(db: &Db) -> BoxStream<'static, Result<bool, DbError>> {
	db.observe(HAS_LIVE, |stmt| stmt.query_row([], |row| row.get("live")))
}

impl DurableTxEntry {
	fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
		Ok(Self {
			id: DurableTxId(row.get("id")?),
			domain: DomainId::new(row.get::<_, String>("domain")?),
			group: row.get::<_, Option<String>>("group_id")?.map(GroupId::new),
			tx_hash: H256(row.get("tx_hash")?),
			mortality: mortality_from_row(row)?,
			status: status_from_row(row)?,
			scanned_to: row.get("scanned_number")?,
		})
	}
}

/// The era of the row's attempt, checked as [`Mortality::new`] checks it.
fn mortality_from_row(row: &Row<'_>) -> rusqlite::Result<Mortality> {
	let birth =
		HashAndNumber { hash: H256(row.get("birth_hash")?), number: row.get("birth_number")? };
	Mortality::new(birth, row.get("period")?).map_err(|error| {
		rusqlite::Error::FromSqlConversionFailure(0, Type::Integer, Box::new(error))
	})
}

/// The block the row's success was recorded at, when it has one.
fn success_from_row(row: &Row<'_>) -> rusqlite::Result<Option<HashAndNumber>> {
	let number: Option<u64> = row.get("success_number")?;
	let hash: Option<[u8; 32]> = row.get("success_hash")?;
	Ok(number
		.zip(hash)
		.map(|(number, hash)| HashAndNumber { hash: H256(hash), number }))
}

impl DurableTxState {
	fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
		Ok(Self { id: DurableTxId(row.get("id")?), status: status_from_row(row)? })
	}
}

/// The row's status, with the evidence its columns hold for it.
fn status_from_row(row: &Row<'_>) -> rusqlite::Result<DurableTxStatus> {
	let name: String = row.get("status")?;
	match name.as_str() {
		"PENDING" => Ok(DurableTxStatus::Pending),
		"PENDING_SUCCESS" => pending_success_from_row(row),
		"FINALIZED_SUCCESS" => Ok(DurableTxStatus::FinalizedSuccess { at: success_from_row(row)? }),
		"FAILURE" => Ok(DurableTxStatus::Failure(row.get("failure_kind")?)),
		other => Err(invalid_status(format!(
			"unknown durable status {other:?}, expected PENDING, PENDING_SUCCESS, \
             FINALIZED_SUCCESS or FAILURE"
		))),
	}
}

/// A `PENDING_SUCCESS` row, which must name the block it was seen at.
fn pending_success_from_row(row: &Row<'_>) -> rusqlite::Result<DurableTxStatus> {
	let at = success_from_row(row)?.ok_or_else(|| {
		invalid_status("PENDING_SUCCESS row without success_number and success_hash".into())
	})?;
	Ok(DurableTxStatus::PendingSuccess { at })
}

fn invalid_status(message: String) -> rusqlite::Error {
	rusqlite::Error::FromSqlConversionFailure(0, Type::Text, message.into())
}

/// The `status` column's value for `status`.
fn status_name(status: DurableTxStatus) -> &'static str {
	match status {
		DurableTxStatus::Pending => "PENDING",
		DurableTxStatus::PendingSuccess { .. } => "PENDING_SUCCESS",
		DurableTxStatus::FinalizedSuccess { .. } => "FINALIZED_SUCCESS",
		DurableTxStatus::Failure(_) => "FAILURE",
	}
}

/// The `failure_kind` column's value for `status`.
fn failure_kind(status: DurableTxStatus) -> Option<FailureKind> {
	match status {
		DurableTxStatus::Failure(kind) => Some(kind),
		_ => None,
	}
}

impl ToSql for FailureKind {
	fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
		Ok(ToSqlOutput::from(match self {
			Self::Expired => "EXPIRED",
			Self::DispatchFailed => "DISPATCH_FAILED",
			Self::Rejected => "REJECTED",
		}))
	}
}

impl FromSql for FailureKind {
	fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
		match value.as_str()? {
			"EXPIRED" => Ok(Self::Expired),
			"DISPATCH_FAILED" => Ok(Self::DispatchFailed),
			"REJECTED" => Ok(Self::Rejected),
			other => Err(FromSqlError::Other(
				format!(
					"unknown durable failure kind {other:?}, expected EXPIRED, \
                     DISPATCH_FAILED or REJECTED"
				)
				.into(),
			)),
		}
	}
}

#[cfg(test)]
mod tests {
	use futures::{StreamExt, executor::block_on};

	use super::*;
	use crate::durable::testing::{block, extrinsic, open_db};

	fn domain() -> DomainId {
		DomainId::new("test")
	}

	fn register(db: &Db, group: Option<GroupId>, tag: u8) -> DurableTxId {
		block_on(
			db.write(move |tx| insert(tx, &domain(), group.as_ref(), &extrinsic(tag, 100, 64))),
		)
		.unwrap()
	}

	fn read<T: Send + 'static>(
		db: &Db,
		f: impl FnOnce(&Connection) -> Result<T, DbError> + Send + 'static,
	) -> T {
		block_on(db.read(f)).unwrap()
	}

	fn write_verdict(db: &Db, observed: DurableTxEntry, verdict: Verdict) -> bool {
		block_on(db.write(move |tx| compare_and_set(tx, &observed, &verdict))).unwrap()
	}

	#[test]
	fn an_inserted_row_reads_back_pending_with_its_attempt() {
		let (_dir, db) = open_db();
		let id = register(&db, Some(GroupId::new("payment")), 7);

		assert_eq!(
			read(&db, move |conn| entry(conn, id)),
			Some(DurableTxEntry {
				id,
				domain: domain(),
				group: Some(GroupId::new("payment")),
				tx_hash: extrinsic(7, 100, 64).extrinsic.hash(),
				mortality: extrinsic(7, 100, 64).mortality,
				status: DurableTxStatus::Pending,
				scanned_to: None,
			})
		);
	}

	#[test]
	fn ids_follow_registration_order() {
		let (_dir, db) = open_db();
		let ids = [register(&db, None, 1), register(&db, None, 2)];

		assert!(ids[0] < ids[1]);
	}

	#[test]
	fn a_missing_row_reads_as_none() {
		let (_dir, db) = open_db();

		assert_eq!(
			read(&db, |conn| Ok((entry(conn, DurableTxId(9))?, status(conn, DurableTxId(9))?))),
			(None, None)
		);
	}

	#[test]
	fn a_verdict_is_written_against_the_status_and_attempt_it_was_derived_from() {
		let (_dir, db) = open_db();
		let id = register(&db, None, 1);
		let observed = read(&db, move |conn| entry(conn, id)).unwrap();
		let at = block(120);

		let wrote =
			write_verdict(&db, observed.clone(), DurableTxStatus::PendingSuccess { at }.into());

		assert!(wrote);
		assert_eq!(
			read(&db, move |conn| entry(conn, id)).unwrap(),
			DurableTxEntry { status: DurableTxStatus::PendingSuccess { at }, ..observed }
		);
	}

	/// A pass and a watch both read a row as Pending; the watch records the
	/// inclusion first. The pass's verdict, derived from the stale status, must
	/// not overwrite it.
	#[test]
	fn a_verdict_is_refused_once_the_status_moved() {
		let (_dir, db) = open_db();
		let id = register(&db, None, 1);
		let observed = read(&db, move |conn| entry(conn, id)).unwrap();
		write_verdict(
			&db,
			observed.clone(),
			DurableTxStatus::PendingSuccess { at: block(120) }.into(),
		);
		let expired = DurableTxStatus::Failure(FailureKind::Expired);

		let wrote = write_verdict(&db, observed, expired.into());

		assert!(!wrote);
		assert_eq!(
			read(&db, move |conn| status(conn, id)),
			Some(DurableTxStatus::PendingSuccess { at: block(120) })
		);
	}

	/// A verdict derived from one attempt must not land on a row whose attempt
	/// has since changed, as it will once a policy rebuilds a transaction under
	/// the same id.
	#[test]
	fn a_verdict_is_refused_for_another_attempt() {
		let (_dir, db) = open_db();
		let id = register(&db, None, 1);
		let mut observed = read(&db, move |conn| entry(conn, id)).unwrap();
		observed.tx_hash = H256::repeat_byte(0xee);

		assert!(!write_verdict(
			&db,
			observed,
			DurableTxStatus::FinalizedSuccess { at: None }.into()
		));
	}

	#[test]
	fn a_terminal_row_is_never_rewritten() {
		let (_dir, db) = open_db();
		let id = register(&db, None, 1);
		let observed = read(&db, move |conn| entry(conn, id)).unwrap();
		write_verdict(&db, observed.clone(), DurableTxStatus::Failure(FailureKind::Expired).into());
		let terminal =
			DurableTxEntry { status: DurableTxStatus::Failure(FailureKind::Expired), ..observed };

		assert!(!write_verdict(
			&db,
			terminal,
			DurableTxStatus::FinalizedSuccess { at: None }.into()
		));
	}

	/// Each status is read back with the evidence it was written with: the
	/// block a success was seen at, and why a transaction failed.
	#[test]
	fn a_status_reads_back_with_its_evidence() {
		let (_dir, db) = open_db();
		let written = [
			DurableTxStatus::PendingSuccess { at: block(120) },
			DurableTxStatus::FinalizedSuccess { at: Some(block(121)) },
			DurableTxStatus::FinalizedSuccess { at: None },
			DurableTxStatus::Failure(FailureKind::Expired),
			DurableTxStatus::Failure(FailureKind::DispatchFailed),
			DurableTxStatus::Failure(FailureKind::Rejected),
		];

		let read_back: Vec<_> = (1..)
			.zip(written)
			.map(|(tag, status)| {
				let id = register(&db, None, tag);
				let observed = read(&db, move |conn| entry(conn, id)).unwrap();
				write_verdict(&db, observed, status.into());
				read(&db, move |conn| super::status(conn, id)).unwrap()
			})
			.collect();

		assert_eq!(read_back, written);
	}

	/// Two domains may pick the same group name, such as an operation id; a
	/// group is only ever read within its domain.
	/// A watch's verdict knows nothing of what recovery already searched, so
	/// writing it must not throw that away.
	#[test]
	fn a_verdict_without_a_cursor_keeps_the_recorded_one() {
		let (_dir, db) = open_db();
		let id = register(&db, None, 7);
		let observed = read(&db, move |conn| entry(conn, id)).unwrap();
		let scanned = Verdict { status: DurableTxStatus::Pending, scanned_to: Some(120) };
		write_verdict(&db, observed, scanned);
		let scanned_entry = read(&db, move |conn| entry(conn, id)).unwrap();

		write_verdict(
			&db,
			scanned_entry,
			DurableTxStatus::PendingSuccess { at: block(125) }.into(),
		);

		let recorded = read(&db, move |conn| entry(conn, id)).unwrap();
		assert_eq!(recorded.scanned_to, Some(120));
	}

	#[test]
	fn a_group_lists_its_own_domain_in_registration_order() {
		let (_dir, db) = open_db();
		let group_id = GroupId::new("payment");
		let first = register(&db, Some(group_id.clone()), 1);
		let second = register(&db, Some(group_id.clone()), 2);
		register(&db, None, 3);
		let other = group_id.clone();
		block_on(db.write(move |tx| {
			insert(tx, &DomainId::new("other"), Some(&other), &extrinsic(4, 100, 64))
		}))
		.unwrap();

		assert_eq!(
			read(&db, move |conn| group(conn, &domain(), &group_id)),
			vec![
				DurableTxState { id: first, status: DurableTxStatus::Pending },
				DurableTxState { id: second, status: DurableTxStatus::Pending },
			]
		);
	}

	#[test]
	fn domain_entries_list_every_row_of_the_domain() {
		let (_dir, db) = open_db();
		let ids = [register(&db, None, 1), register(&db, None, 2)];

		let listed = read(&db, |conn| domain_entries(conn, &domain()));

		assert_eq!(listed.iter().map(|entry| entry.id).collect::<Vec<_>>(), ids);
	}

	#[test]
	fn only_domains_with_a_transaction_awaiting_a_verdict_are_live() {
		let (_dir, db) = open_db();
		assert_eq!(read(&db, live_domains), Vec::<DomainId>::new());
		let id = register(&db, None, 1);
		assert_eq!(read(&db, live_domains), vec![domain()]);

		let observed = read(&db, move |conn| entry(conn, id)).unwrap();
		write_verdict(&db, observed, DurableTxStatus::FinalizedSuccess { at: None }.into());

		assert_eq!(read(&db, live_domains), Vec::<DomainId>::new());
	}

	#[test]
	fn observed_queries_follow_commits() {
		let (_dir, db) = open_db();
		let group_id = GroupId::new("payment");
		let mut live = observe_has_live(&db);
		let mut grouped = observe_group(&db, domain(), group_id.clone());
		assert!(!next(&mut live));
		assert_eq!(next(&mut grouped), vec![]);

		let id = register(&db, Some(group_id), 1);
		let mut status = observe_status(&db, id);

		assert!(next(&mut live));
		assert_eq!(
			next(&mut grouped),
			vec![DurableTxState { id, status: DurableTxStatus::Pending }]
		);
		assert_eq!(next(&mut status), Some(DurableTxStatus::Pending));
	}

	fn next<T>(stream: &mut BoxStream<'static, Result<T, DbError>>) -> T {
		block_on(stream.next()).unwrap().unwrap()
	}

	/// The table carries step 4's status already; until that code exists a
	/// row in it is reported rather than misread.
	#[test]
	fn a_status_the_engine_does_not_model_is_a_read_error() {
		let (_dir, db) = open_db();
		let id = register(&db, None, 1);
		block_on(db.write(move |tx| {
			tx.execute(
				"UPDATE durable_tx SET status = 'PENDING_SUBMISSION' WHERE id = ?1",
				[id.0],
			)?;
			Ok(())
		}))
		.unwrap();

		assert!(block_on(db.read(move |conn| status(conn, id))).is_err());
	}
}
