//! B2 · The ledger: what only this layer knows about its purses and assets.
//!
//! It stores no asset status (invariant 1): whether an asset is in use or
//! spent comes from the durability layer's records and C1's rows, and B1
//! computes it from a [`Snapshot`]. Every write takes the caller's store
//! transaction, so it commits with whatever else the caller writes.

use core::time::Duration;
use std::collections::{BTreeMap, HashMap};

use parity_scale_codec::{Decode, Encode};
use rusqlite::{Connection, OptionalExtension, Transaction, named_params, types::Type};

use super::{
	Exponent, HeldBackSpend, MemberThreshold, PrivacyPolicy, PurseId, ReadyRule, RecycleAge,
	operation::{self, OpKey, OperationRecord},
	registration::{self, Registrations},
};
use crate::{installation::InstallationId, store::DbError};

/// One holding the layer tracks, named by its derivation indices, never by
/// its account or member key (C1 invariant 6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Encode, Decode)]
pub enum Asset {
	/// A coin this layer derived.
	Coin {
		/// The purse it belongs to.
		purse: PurseId,
		/// The installation it was allocated under.
		installation: InstallationId,
		/// Its index within the purse and installation.
		index: u32,
	},
	/// A recycler entry this layer derived.
	Entry {
		/// The purse it belongs to.
		purse: PurseId,
		/// The installation it was allocated under.
		installation: InstallationId,
		/// Its index within the purse and installation.
		index: u32,
	},
	/// A coin a peer sent, which has no index here and is only ever the
	/// input of its import.
	PeerCoin([u8; 32]),
}

impl Asset {
	/// The purse a coin or entry of this layer belongs to; none for a peer's
	/// coin.
	pub fn purse(&self) -> Option<PurseId> {
		match self {
			Self::Coin { purse, .. } | Self::Entry { purse, .. } => Some(*purse),
			Self::PeerCoin(_) => None,
		}
	}

	/// The installation a coin or entry of this layer was allocated under.
	pub fn installation(&self) -> Option<InstallationId> {
		match self {
			Self::Coin { installation, .. } | Self::Entry { installation, .. } => Some(*installation),
			Self::PeerCoin(_) => None,
		}
	}
}

/// Which tree an allocated asset comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode)]
pub enum AssetKind {
	/// A coin.
	Coin,
	/// A recycler entry.
	Entry,
}

/// A coin's or entry's identity and its own facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode)]
pub struct AssetRecord {
	/// The asset.
	pub asset: Asset,
	/// Its denomination.
	pub exponent: Exponent,
}

/// A purse's state, which only moves forward (invariant 7).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode)]
pub enum PurseState {
	/// It holds value and receives it.
	Open,
	/// Its value is being drained into `drain_into`.
	Deleting {
		/// The purse its value goes to.
		drain_into: PurseId,
	},
	/// It is drained, into `drained_into`.
	Closed {
		/// The purse its value went to.
		drained_into: PurseId,
	},
}

/// A purse's metadata and state.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub struct PurseRecord {
	/// Its identifier.
	pub id: PurseId,
	/// Its name.
	pub name: String,
	/// When it was created, in Unix seconds.
	pub created: u64,
	/// Opaque bytes its creator attached, which this layer never reads.
	pub tag: Vec<u8>,
	/// Its state.
	pub state: PurseState,
}

/// A finalized block: its number and hash.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode)]
pub struct BlockRef {
	/// The block's number.
	pub number: u32,
	/// The block's hash.
	pub hash: [u8; 32],
}

/// One unload-token identity: a slot of a period's allowance (D1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Encode, Decode)]
pub struct TokenId {
	/// The period.
	pub period: u32,
	/// The slot, that is the counter value, within the period.
	pub slot: u32,
}

/// What F1 read, all at one block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Facts {
	/// The block every read was made at.
	pub at: BlockRef,
	/// The reads.
	pub reads: Vec<(Subject, Read)>,
}

/// What a read is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Encode, Decode)]
pub enum Subject {
	/// A coin or an entry.
	Asset(Asset),
	/// An unload-token identity.
	Token(TokenId),
}

/// The outcome of one read.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub enum Read {
	/// The read did not return (invariant 6).
	Failed,
	/// A coin's account.
	Coin {
		/// The coin in the account; none when the account is empty.
		now: Option<CoinOnChain>,
		/// For an exported coin, the transaction that emptied its account.
		emptied_by: Option<Emptied>,
	},
	/// An entry's ring membership; none when its key is in no ring.
	Entry(Option<EntryOnChain>),
	/// Whether the chain records the token as consumed.
	Token {
		/// It does.
		consumed: bool,
	},
}

/// The transaction that emptied an exported coin's account, as F1 found it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode)]
pub struct Emptied {
	/// Its hash.
	pub by: [u8; 32],
	/// Whether the block that included it was finalized at the read.
	pub stable: bool,
}

/// A coin as the chain shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode)]
pub struct CoinOnChain {
	/// Its exponent.
	pub exponent: Exponent,
	/// Its age.
	pub age: u32,
	/// When its lock after a failed dispatch ends, in Unix seconds.
	pub locked_until: Option<u64>,
}

/// An entry's place in its ring, as the chain shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode)]
pub struct EntryOnChain {
	/// The ring's index.
	pub ring: u32,
	/// The ring's current revision.
	pub revision: u32,
	/// The page of the ring the entry's key is on.
	pub page: u32,
	/// The key's position within that page.
	pub position: u32,
	/// How many members the ring has.
	pub members: u32,
	/// How many members the ring's root includes.
	pub included: u32,
	/// Whether the ring accepts no more members.
	pub closed: bool,
	/// When the ring expires, in Unix seconds, once it is closed.
	pub expires_at: Option<u64>,
	/// When the entry's alias lock ends, in Unix seconds.
	pub locked_until: Option<u64>,
}

impl EntryOnChain {
	/// Whether the ring's root includes the entry's key: the included count
	/// passes the key's place in the whole ring, not just in its page
	/// (B1 invariant 4).
	pub fn key_included(&self, page_size: u32) -> bool {
		let place = u64::from(self.page) * u64::from(page_size) + u64::from(self.position);
		u64::from(self.included) > place
	}
}

/// Whether the chain has reported on a subject: B1's third axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Encode, Decode)]
pub enum Observed {
	/// It never has.
	Never,
	/// It last reported at this block.
	At(BlockRef),
	/// Reads have failed since it last reported, at this block.
	FailedSince(BlockRef),
}

/// What the ledger stores about a subject's chain state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
	/// Whether, and when, the chain reported.
	pub observed: Observed,
	/// The last value the chain reported; never a failed read.
	pub value: Option<Read>,
	/// For an entry, when its key was first seen included in its ring's
	/// root, in Unix seconds.
	pub included_at: Option<u64>,
}

/// A disclosure mark's phase (invariant 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
	/// Written when an export reserved the asset; deleted at start.
	Uncommitted,
	/// Written once the secret is durable; never deleted.
	Committed,
}

/// How far a reconstruction has scanned one installation. `resume` is F4's
/// and opaque here.
#[derive(Clone, Debug, Default, PartialEq, Eq, Encode, Decode)]
pub struct ScanProgress {
	/// Whether its first scan has completed.
	pub first_scan_done: bool,
	/// Where the next scan resumes.
	pub resume: Vec<u8>,
}

/// Why a purse deletion or an installation's start was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, derive_more::Display, derive_more::Error)]
pub enum LedgerError {
	/// The main purse cannot be deleted.
	#[display("the main purse cannot be deleted")]
	MainPurse,
	/// The purse already has a deletion open.
	#[display("the purse already has a deletion open")]
	DeletionOpen,
	/// The purse, or the purse to drain into, is not open, or they are the
	/// same purse.
	#[display("no such open purse")]
	PurseNotFound,
	/// The purse backup does not decode.
	#[display("the purse backup does not decode")]
	UndecodableStore,
}

const POLICY: &str = "policy";
const OWN_OPERATIONS: &str = "own_operations";

/// One committed state of the ledger, read in one read transaction. Every
/// read here comes from that state, so no two reads fall on both sides of a
/// commit (invariant 3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
	purses: BTreeMap<PurseId, PurseRecord>,
	assets: BTreeMap<Asset, AssetRecord>,
	observations: HashMap<Subject, Observation>,
	marks: HashMap<Asset, Mark>,
	policy: PrivacyPolicy,
	installations: Vec<(InstallationId, ScanProgress)>,
	current_installation: Option<InstallationId>,
	operations: BTreeMap<OpKey, OperationRecord>,
	registrations: Registrations,
}

impl Snapshot {
	/// A purse's metadata and state.
	pub fn purse(&self, id: PurseId) -> Option<&PurseRecord> {
		self.purses.get(&id)
	}

	/// Every purse ever created, closed ones included.
	pub fn purses(&self) -> impl Iterator<Item = &PurseRecord> {
		self.purses.values()
	}

	/// The purse itself while it is open; otherwise the open purse its
	/// deletion drains into, followed to the end (invariant 7).
	pub fn resolve_purse(&self, id: PurseId) -> PurseId {
		let mut current = id;
		for _ in 0..=self.purses.len() {
			match self.purses.get(&current).map(|purse| purse.state) {
				Some(PurseState::Deleting { drain_into }) => current = drain_into,
				Some(PurseState::Closed { drained_into }) => current = drained_into,
				Some(PurseState::Open) | None => return current,
			}
		}
		current
	}

	/// One asset's identity.
	pub fn asset(&self, asset: &Asset) -> Option<&AssetRecord> {
		self.assets.get(asset)
	}

	/// The assets a purse holds: those under the current installation, and
	/// for the main purse also every asset under an earlier installation,
	/// whatever purse it was found under (invariant 5).
	pub fn assets(&self, purse: PurseId) -> impl Iterator<Item = &AssetRecord> {
		let current = self.current_installation;
		self.assets.values().filter(move |record| {
			let earlier = record.asset.installation() != current;
			if earlier { purse == PurseId::MAIN } else { record.asset.purse() == Some(purse) }
		})
	}

	/// Every coin and entry the layer knows.
	pub fn all_assets(&self) -> impl Iterator<Item = &AssetRecord> {
		self.assets.values()
	}

	/// What the chain last showed for `subject`.
	pub fn observation(&self, subject: &Subject) -> Option<&Observation> {
		self.observations.get(subject)
	}

	/// The asset's disclosure mark.
	pub fn mark(&self, asset: &Asset) -> Option<Mark> {
		self.marks.get(asset).copied()
	}

	/// The stored privacy policy.
	pub fn policy(&self) -> &PrivacyPolicy {
		&self.policy
	}

	/// The installation the layer allocates into.
	pub fn current_installation(&self) -> Option<InstallationId> {
		self.current_installation
	}

	/// Every installation known, in the order learned, with its scan
	/// progress.
	pub fn installations(&self) -> &[(InstallationId, ScanProgress)] {
		&self.installations
	}

	/// One operation as B4 stores it.
	pub fn operation(&self, key: &OpKey) -> Option<&OperationRecord> {
		self.operations.get(key)
	}

	/// Every operation B4 stores.
	pub fn operations(&self) -> impl Iterator<Item = &OperationRecord> {
		self.operations.values()
	}

	/// C1's rows of every record, with the durability layer's state of each.
	pub fn registrations(&self) -> &Registrations {
		&self.registrations
	}
}

/// The ledger in one state: as committed now, read in one read
/// transaction, or, inside the caller's open transaction, as that
/// transaction sees it.
pub fn snapshot(conn: &Connection) -> Result<Snapshot, DbError> {
	if conn.is_autocommit() {
		let read = conn.unchecked_transaction()?;
		return load(&read);
	}
	load(conn)
}

fn load(read: &Connection) -> Result<Snapshot, DbError> {
	let purses = rows(read, "SELECT record FROM coinage_purse", |row| decode::<PurseRecord>(row, 0))?
		.into_iter()
		.map(|purse| (purse.id, purse))
		.collect();
	let assets = rows(read, "SELECT record FROM coinage_asset", |row| decode::<AssetRecord>(row, 0))?
		.into_iter()
		.map(|record| (record.asset, record))
		.collect();
	let observations = rows(
		read,
		"SELECT subject, observed, value, included_at FROM coinage_observation",
		|row| Ok((decode::<Subject>(row, 0)?, observation_columns(row, 1)?)),
	)?
	.into_iter()
	.collect();
	let marks = rows(read, "SELECT asset, committed FROM coinage_mark", |row| {
		let mark = if row.get::<_, bool>(1)? { Mark::Committed } else { Mark::Uncommitted };
		Ok((decode::<Asset>(row, 0)?, mark))
	})?
	.into_iter()
	.collect();
	let installations = rows(
		read,
		"SELECT id, progress, current FROM coinage_installation ORDER BY learned",
		|row| Ok((decode::<InstallationId>(row, 0)?, decode::<ScanProgress>(row, 1)?, row.get::<_, bool>(2)?)),
	)?;
	let current_installation = installations.iter().find(|(_, _, current)| *current).map(|(id, _, _)| *id);
	let policy = read_policy(read)?;
	let operations = operation::all(read)?.into_iter().map(|record| (record.key.clone(), record)).collect();
	let registrations = registration::load(read)?;
	Ok(Snapshot {
		purses,
		assets,
		observations,
		marks,
		policy,
		installations: installations.into_iter().map(|(id, progress, _)| (id, progress)).collect(),
		current_installation,
		operations,
		registrations,
	})
}

/// Creates a product purse, open, with the identifier one past the largest
/// product purse identifier the layer knows of: every purse it has created,
/// those a backup restored, and any a reconstruction found.
pub fn create_purse(tx: &Transaction<'_>, name: String, tag: Vec<u8>, now: u64) -> Result<PurseId, DbError> {
	let created = rows(tx, "SELECT id FROM coinage_purse", |row| row.get::<_, i64>(0))?
		.into_iter()
		.map(|id| id as u32);
	let found = rows(tx, "SELECT asset FROM coinage_asset", |row| decode::<Asset>(row, 0))?
		.into_iter()
		.filter_map(|asset| asset.purse())
		.map(|purse| purse.0);
	let next = created
		.chain(found)
		.filter(|id| *id != PurseId::MAIN.0)
		.map(|id| id + 1)
		.max()
		.unwrap_or(0);
	let purse = PurseRecord { id: PurseId(next), name, created: now, tag, state: PurseState::Open };
	write_purse(tx, &purse)?;
	Ok(purse.id)
}

/// Moves the open product purse `purse` to `Deleting`, draining into the
/// open purse `drain_into`.
pub fn begin_delete(
	tx: &Transaction<'_>,
	purse: PurseId,
	drain_into: PurseId,
) -> Result<Result<(), LedgerError>, DbError> {
	if purse == PurseId::MAIN {
		return Ok(Err(LedgerError::MainPurse));
	}
	let Some(mut record) = read_purse(tx, purse)? else {
		return Ok(Err(LedgerError::PurseNotFound));
	};
	match record.state {
		PurseState::Deleting { .. } => return Ok(Err(LedgerError::DeletionOpen)),
		PurseState::Closed { .. } => return Ok(Err(LedgerError::PurseNotFound)),
		PurseState::Open => {},
	}
	let target_open = read_purse(tx, drain_into)?.is_some_and(|target| target.state == PurseState::Open);
	if drain_into == purse || !target_open {
		return Ok(Err(LedgerError::PurseNotFound));
	}
	record.state = PurseState::Deleting { drain_into };
	write_purse(tx, &record)?;
	Ok(Ok(()))
}

/// Moves a `Deleting` purse to `Closed`.
pub fn close_purse(tx: &Transaction<'_>, purse: PurseId) -> Result<(), DbError> {
	if let Some(mut record) = read_purse(tx, purse)?
		&& let PurseState::Deleting { drain_into } = record.state
	{
		record.state = PurseState::Closed { drained_into: drain_into };
		write_purse(tx, &record)?;
	}
	Ok(())
}

/// Records one fresh asset per output in `purse` under the current
/// installation, at the next indices, and returns them in order. Indices are
/// never reissued (invariant 4).
pub fn allocate(
	tx: &Transaction<'_>,
	purse: PurseId,
	kind: AssetKind,
	outputs: &[Exponent],
) -> Result<Vec<Asset>, DbError> {
	let installation = current_installation(tx)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
	let kind_code = kind as i64;
	let next: i64 = tx
		.query_row(
			"SELECT next FROM coinage_counter WHERE purse = :purse AND installation = :installation AND kind = :kind",
			named_params! { ":purse": purse.0, ":installation": installation.encode(), ":kind": kind_code },
			|row| row.get(0),
		)
		.optional()?
		.unwrap_or(0);
	let mut allocated = Vec::with_capacity(outputs.len());
	for (offset, exponent) in outputs.iter().enumerate() {
		let index = u32::try_from(next + offset as i64).map_err(|_| conversion_error(0, "the purse has used every index"))?;
		let asset = match kind {
			AssetKind::Coin => Asset::Coin { purse, installation, index },
			AssetKind::Entry => Asset::Entry { purse, installation, index },
		};
		write_asset(tx, &AssetRecord { asset, exponent: *exponent })?;
		allocated.push(asset);
	}
	tx.execute(
		"INSERT INTO coinage_counter (purse, installation, kind, next) VALUES (:purse, :installation, :kind, :next)
		 ON CONFLICT (purse, installation, kind) DO UPDATE SET next = excluded.next",
		named_params! {
			":purse": purse.0,
			":installation": installation.encode(),
			":kind": kind_code,
			":next": next + outputs.len() as i64,
		},
	)?;
	Ok(allocated)
}

/// Records assets a reconstruction found under their own installations,
/// without advancing any counter.
pub fn adopt(tx: &Transaction<'_>, found: &[AssetRecord]) -> Result<(), DbError> {
	for record in found {
		tx.execute(
			"INSERT OR IGNORE INTO coinage_asset (asset, record) VALUES (:asset, :record)",
			named_params! { ":asset": record.asset.encode(), ":record": record.encode() },
		)?;
	}
	Ok(())
}

/// Stores what F1 read at `facts.at`. A value read at a block not older than
/// the stored one replaces it; a failed read, or one at an older block,
/// leaves the stored value and marks the subject *read failed since* its
/// block (invariant 6). The first time an entry's key is seen included in its
/// ring's root, its inclusion instant becomes `now`.
pub fn observe(tx: &Transaction<'_>, facts: &Facts, now: u64, page_size: u32) -> Result<(), DbError> {
	for (subject, read) in &facts.reads {
		let stored = read_observation(tx, subject)?;
		let stored_block = match stored.observed {
			Observed::Never => None,
			Observed::At(block) | Observed::FailedSince(block) => Some(block),
		};
		let stale = stored_block.is_some_and(|block| facts.at.number < block.number);
		let observation = if *read == Read::Failed || stale {
			Observation {
				observed: stored_block.map_or(Observed::Never, Observed::FailedSince),
				..stored
			}
		} else {
			let included_now = matches!(read, Read::Entry(Some(entry)) if entry.key_included(page_size));
			Observation {
				observed: Observed::At(facts.at),
				value: Some(read.clone()),
				included_at: stored.included_at.or(included_now.then_some(now)),
			}
		};
		tx.execute(
			"INSERT INTO coinage_observation (subject, observed, value, included_at)
			 VALUES (:subject, :observed, :value, :included_at)
			 ON CONFLICT (subject) DO UPDATE SET
			 observed = excluded.observed, value = excluded.value, included_at = excluded.included_at",
			named_params! {
				":subject": subject.encode(),
				":observed": observation.observed.encode(),
				":value": observation.value.as_ref().map(Encode::encode),
				":included_at": observation.included_at.map(|at| at as i64),
			},
		)?;
	}
	Ok(())
}

/// Stores the secrets of the peer coins an import claims, by account.
pub fn keep_secrets(tx: &Transaction<'_>, secrets: &[([u8; 32], super::CoinSecret)]) -> Result<(), DbError> {
	for (account, secret) in secrets {
		tx.execute(
			"INSERT OR REPLACE INTO coinage_peer_secret (account, secret) VALUES (:account, :secret)",
			named_params! { ":account": account.as_slice(), ":secret": secret.0.as_slice() },
		)?;
	}
	Ok(())
}

/// The stored secret of a peer coin's account.
pub fn secret(conn: &Connection, account: &[u8; 32]) -> Result<Option<super::CoinSecret>, DbError> {
	let bytes: Option<Vec<u8>> = conn
		.query_row(
			"SELECT secret FROM coinage_peer_secret WHERE account = :account",
			named_params! { ":account": account.as_slice() },
			|row| row.get(0),
		)
		.optional()?;
	bytes
		.map(|bytes| {
			<[u8; 64]>::try_from(bytes.as_slice())
				.map(super::CoinSecret)
				.map_err(|_| DbError::Sqlite(conversion_error(0, "a coin secret is 64 bytes")))
		})
		.transpose()
}

/// Deletes the secrets of peer coins whose claims are final.
pub fn forget_secrets(tx: &Transaction<'_>, accounts: &[[u8; 32]]) -> Result<(), DbError> {
	for account in accounts {
		tx.execute(
			"DELETE FROM coinage_peer_secret WHERE account = :account",
			named_params! { ":account": account.as_slice() },
		)?;
	}
	Ok(())
}

/// Writes uncommitted disclosure marks, or returns the first asset that is
/// already marked or held as an input (C1); nothing is written then. For a
/// coin an export hands out as it is, which no registration holds, this is
/// what keeps another operation from selecting it (invariant 2).
pub fn mark_disclosed(tx: &Transaction<'_>, assets: &[Asset]) -> Result<Result<(), Asset>, DbError> {
	for asset in assets {
		if mark(tx, asset)?.is_some() || registration::held(tx, asset)? {
			return Ok(Err(*asset));
		}
	}
	for asset in assets {
		tx.execute(
			"INSERT INTO coinage_mark (asset, committed) VALUES (:asset, 0)",
			named_params! { ":asset": asset.encode() },
		)?;
	}
	Ok(Ok(()))
}

/// Makes disclosure marks committed.
pub fn commit_disclosed(tx: &Transaction<'_>, assets: &[Asset]) -> Result<(), DbError> {
	for asset in assets {
		tx.execute(
			"UPDATE coinage_mark SET committed = 1 WHERE asset = :asset",
			named_params! { ":asset": asset.encode() },
		)?;
	}
	Ok(())
}

/// Deletes uncommitted disclosure marks; committed ones stay.
pub fn release_disclosed(tx: &Transaction<'_>, assets: &[Asset]) -> Result<(), DbError> {
	for asset in assets {
		tx.execute(
			"DELETE FROM coinage_mark WHERE asset = :asset AND committed = 0",
			named_params! { ":asset": asset.encode() },
		)?;
	}
	Ok(())
}

/// Every asset with an uncommitted disclosure mark.
pub fn uncommitted(conn: &Connection) -> Result<Vec<Asset>, DbError> {
	rows(conn, "SELECT asset FROM coinage_mark WHERE committed = 0", |row| decode::<Asset>(row, 0))
}

/// The installation the layer allocates into.
pub fn current_installation(conn: &Connection) -> Result<Option<InstallationId>, DbError> {
	let bytes: Option<Vec<u8>> = conn
		.query_row("SELECT id FROM coinage_installation WHERE current = 1", [], |row| row.get(0))
		.optional()?;
	bytes.map(|bytes| decode_bytes(&bytes, 0)).transpose().map_err(DbError::from)
}

/// Records a fresh installation as current, stores the initial policy,
/// restores the purse backup if one is given, and records the main purse,
/// which exists as soon as the seed does (G3 invariants 2 and 3).
pub fn mint_installation(
	tx: &Transaction<'_>,
	id: InstallationId,
	policy: &PrivacyPolicy,
	backup: Option<&[u8]>,
	now: u64,
) -> Result<Result<(), LedgerError>, DbError> {
	let purses = match backup.map(|mut bytes| Vec::<PurseRecord>::decode(&mut bytes)) {
		Some(Err(_)) => return Ok(Err(LedgerError::UndecodableStore)),
		Some(Ok(purses)) => purses,
		None => Vec::new(),
	};
	tx.execute("UPDATE coinage_installation SET current = 0", [])?;
	tx.execute(
		"INSERT INTO coinage_installation (id, learned, current, progress)
		 VALUES (:id, (SELECT COALESCE(MAX(learned), 0) + 1 FROM coinage_installation), 1, :progress)",
		named_params! { ":id": id.encode(), ":progress": ScanProgress::default().encode() },
	)?;
	set_policy(tx, policy)?;
	let main = PurseRecord { id: PurseId::MAIN, name: String::new(), created: now, tag: Vec::new(), state: PurseState::Open };
	for purse in purses.into_iter().chain([main]) {
		if read_purse(tx, purse.id)?.is_none() {
			write_purse(tx, &purse)?;
		}
	}
	Ok(Ok(()))
}

/// Records earlier installations a reconstruction learned. A known
/// installation is never removed (invariant 4).
pub fn add_installations(tx: &Transaction<'_>, ids: &[InstallationId]) -> Result<(), DbError> {
	for id in ids {
		tx.execute(
			"INSERT OR IGNORE INTO coinage_installation (id, learned, current, progress)
			 VALUES (:id, (SELECT COALESCE(MAX(learned), 0) + 1 FROM coinage_installation), 0, :progress)",
			named_params! { ":id": id.encode(), ":progress": ScanProgress::default().encode() },
		)?;
	}
	Ok(())
}

/// Stores F4's scan progress for an installation.
pub fn record_scan(tx: &Transaction<'_>, installation: InstallationId, progress: &ScanProgress) -> Result<(), DbError> {
	tx.execute(
		"UPDATE coinage_installation SET progress = :progress WHERE id = :id",
		named_params! { ":id": installation.encode(), ":progress": progress.encode() },
	)?;
	Ok(())
}

/// Replaces the stored policy. Nothing else stored changes with it
/// (invariant 1).
pub fn set_policy(tx: &Transaction<'_>, policy: &PrivacyPolicy) -> Result<(), DbError> {
	write_setting(tx, POLICY, &StoredPolicy::from(policy).encode())
}

/// The next id for one of the layer's own operations (F2 invariant 3).
pub fn next_operation_id(tx: &Transaction<'_>) -> Result<[u8; 32], DbError> {
	let stored: Option<Vec<u8>> = tx
		.query_row(
			"SELECT value FROM coinage_setting WHERE name = :name",
			named_params! { ":name": OWN_OPERATIONS },
			|row| row.get(0),
		)
		.optional()?;
	let counter = match stored {
		Some(bytes) => decode_bytes::<u64>(&bytes, 0)? + 1,
		None => 0,
	};
	write_setting(tx, OWN_OPERATIONS, &counter.encode())?;
	let mut id = [0u8; 32];
	id[..8].copy_from_slice(&counter.to_le_bytes());
	Ok(id)
}

/// `SCALE(Vec<PurseRecord>)` of every purse ever created, closed ones
/// included.
pub fn purse_backup(conn: &Connection) -> Result<Vec<u8>, DbError> {
	let purses = rows(conn, "SELECT record FROM coinage_purse ORDER BY id", |row| decode::<PurseRecord>(row, 0))?;
	Ok(purses.encode())
}

/// The policy as stored: durations in seconds.
#[derive(Encode, Decode)]
struct StoredPolicy {
	ready_when: Vec<(StoredThreshold, u64)>,
	recycle_at: StoredRecycleAge,
	max_unavailable_share: u8,
	held_back_for_user: u8,
	held_back_for_products: u8,
	move_through_recycler: bool,
}

#[derive(Encode, Decode)]
enum StoredThreshold {
	Count(u32),
	PercentOfRingSize(u8),
}

#[derive(Encode, Decode)]
enum StoredRecycleAge {
	Age(u32),
	CutoffDividedBy(u32),
}

impl From<&PrivacyPolicy> for StoredPolicy {
	fn from(policy: &PrivacyPolicy) -> Self {
		let held_back = |setting: HeldBackSpend| match setting {
			HeldBackSpend::Never => 0,
			HeldBackSpend::AfterConfirmation => 1,
			HeldBackSpend::Always => 2,
		};
		Self {
			ready_when: policy
				.ready_when
				.iter()
				.map(|rule| {
					let threshold = match rule.members {
						MemberThreshold::Count(count) => StoredThreshold::Count(count),
						MemberThreshold::PercentOfRingSize(percent) => StoredThreshold::PercentOfRingSize(percent),
					};
					(threshold, rule.dwell.as_secs())
				})
				.collect(),
			recycle_at: match policy.recycle_at {
				RecycleAge::Age(age) => StoredRecycleAge::Age(age),
				RecycleAge::CutoffDividedBy(divisor) => StoredRecycleAge::CutoffDividedBy(divisor),
			},
			max_unavailable_share: policy.max_unavailable_share,
			held_back_for_user: held_back(policy.held_back_for_user),
			held_back_for_products: held_back(policy.held_back_for_products),
			move_through_recycler: policy.move_through_recycler,
		}
	}
}

impl From<StoredPolicy> for PrivacyPolicy {
	fn from(stored: StoredPolicy) -> Self {
		let held_back = |code: u8| match code {
			0 => HeldBackSpend::Never,
			1 => HeldBackSpend::AfterConfirmation,
			_ => HeldBackSpend::Always,
		};
		Self {
			ready_when: stored
				.ready_when
				.into_iter()
				.map(|(threshold, dwell)| ReadyRule {
					members: match threshold {
						StoredThreshold::Count(count) => MemberThreshold::Count(count),
						StoredThreshold::PercentOfRingSize(percent) => MemberThreshold::PercentOfRingSize(percent),
					},
					dwell: Duration::from_secs(dwell),
				})
				.collect(),
			recycle_at: match stored.recycle_at {
				StoredRecycleAge::Age(age) => RecycleAge::Age(age),
				StoredRecycleAge::CutoffDividedBy(divisor) => RecycleAge::CutoffDividedBy(divisor),
			},
			max_unavailable_share: stored.max_unavailable_share,
			held_back_for_user: held_back(stored.held_back_for_user),
			held_back_for_products: held_back(stored.held_back_for_products),
			move_through_recycler: stored.move_through_recycler,
		}
	}
}

/// The stored policy; the *Fastest* preset until an installation stores one.
fn read_policy(conn: &Connection) -> Result<PrivacyPolicy, DbError> {
	let stored: Option<Vec<u8>> = conn
		.query_row(
			"SELECT value FROM coinage_setting WHERE name = :name",
			named_params! { ":name": POLICY },
			|row| row.get(0),
		)
		.optional()?;
	match stored {
		Some(bytes) => Ok(decode_bytes::<StoredPolicy>(&bytes, 0)?.into()),
		None => Ok(PrivacyPolicy::fastest()),
	}
}

fn write_setting(tx: &Transaction<'_>, name: &str, value: &[u8]) -> Result<(), DbError> {
	tx.execute(
		"INSERT INTO coinage_setting (name, value) VALUES (:name, :value)
		 ON CONFLICT (name) DO UPDATE SET value = excluded.value",
		named_params! { ":name": name, ":value": value },
	)?;
	Ok(())
}

fn read_purse(conn: &Connection, id: PurseId) -> Result<Option<PurseRecord>, DbError> {
	let bytes: Option<Vec<u8>> = conn
		.query_row(
			"SELECT record FROM coinage_purse WHERE id = :id",
			named_params! { ":id": id.0 },
			|row| row.get(0),
		)
		.optional()?;
	Ok(bytes.map(|bytes| decode_bytes(&bytes, 0)).transpose()?)
}

fn write_purse(tx: &Transaction<'_>, purse: &PurseRecord) -> Result<(), DbError> {
	tx.execute(
		"INSERT INTO coinage_purse (id, record) VALUES (:id, :record)
		 ON CONFLICT (id) DO UPDATE SET record = excluded.record",
		named_params! { ":id": purse.id.0, ":record": purse.encode() },
	)?;
	Ok(())
}

/// Fixes the denomination of an import's output at the exponent F1 first
/// observed the claimed coin at (B1): the one asset whose denomination is
/// not known when it is allocated, and that nothing reads for more than a
/// provisional value before its claim can be built.
pub fn fix_import_output(tx: &Transaction<'_>, output: Asset, exponent: Exponent) -> Result<(), DbError> {
	let record = AssetRecord { asset: output, exponent };
	tx.execute(
		"UPDATE coinage_asset SET record = :record WHERE asset = :asset",
		named_params! { ":asset": record.asset.encode(), ":record": record.encode() },
	)?;
	Ok(())
}

fn write_asset(tx: &Transaction<'_>, record: &AssetRecord) -> Result<(), DbError> {
	tx.execute(
		"INSERT INTO coinage_asset (asset, record) VALUES (:asset, :record)",
		named_params! { ":asset": record.asset.encode(), ":record": record.encode() },
	)?;
	Ok(())
}

/// The asset's disclosure mark, as committed now.
pub fn mark(conn: &Connection, asset: &Asset) -> Result<Option<Mark>, DbError> {
	let committed: Option<bool> = conn
		.query_row(
			"SELECT committed FROM coinage_mark WHERE asset = :asset",
			named_params! { ":asset": asset.encode() },
			|row| row.get(0),
		)
		.optional()?;
	Ok(committed.map(|committed| if committed { Mark::Committed } else { Mark::Uncommitted }))
}

fn read_observation(conn: &Connection, subject: &Subject) -> Result<Observation, DbError> {
	let stored = conn
		.query_row(
			"SELECT observed, value, included_at FROM coinage_observation WHERE subject = :subject",
			named_params! { ":subject": subject.encode() },
			|row| observation_columns(row, 0),
		)
		.optional()?;
	Ok(stored.unwrap_or(Observation { observed: Observed::Never, value: None, included_at: None }))
}

/// The observation in the columns `observed, value, included_at` of `row`,
/// starting at `first`.
fn observation_columns(row: &rusqlite::Row<'_>, first: usize) -> rusqlite::Result<Observation> {
	let value = match row.get::<_, Option<Vec<u8>>>(first + 1)? {
		Some(bytes) => Some(decode_bytes::<Read>(&bytes, first + 1)?),
		None => None,
	};
	Ok(Observation {
		observed: decode::<Observed>(row, first)?,
		value,
		included_at: row.get::<_, Option<i64>>(first + 2)?.map(|at| at as u64),
	})
}

/// Every row of `sql`, mapped.
fn rows<T>(
	conn: &Connection,
	sql: &str,
	mut map: impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
) -> Result<Vec<T>, DbError> {
	let mut statement = conn.prepare_cached(sql)?;
	let mapped = statement.query_map([], |row| map(row))?;
	Ok(mapped.collect::<Result<_, _>>()?)
}

/// Column `index` of `row`, SCALE-decoded.
fn decode<T: Decode>(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<T> {
	let bytes: Vec<u8> = row.get(index)?;
	decode_bytes(&bytes, index)
}

fn decode_bytes<T: Decode>(mut bytes: &[u8], column: usize) -> rusqlite::Result<T> {
	T::decode(&mut bytes).map_err(|error| rusqlite::Error::FromSqlConversionFailure(column, Type::Blob, Box::new(error)))
}

fn conversion_error(column: usize, reason: &'static str) -> rusqlite::Error {
	rusqlite::Error::FromSqlConversionFailure(column, Type::Integer, reason.into())
}

#[cfg(test)]
mod tests;
