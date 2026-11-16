//! G2 · Subscriptions and events: the operation status, balance, export
//! status and event streams.
//!
//! Every stream re-reads the store whenever something it reads may have
//! changed: a commit to the records, marks, operations or purses (seen
//! through the store's observed queries, so a status the durability layer
//! writes reaches it), and every write, `run_due` call and new block of the
//! layer itself, which also covers what changes with the clock alone
//! (invariant 1). A value stream emits its current value first and then only
//! values that differ from the last one (invariant 4).

use core::future::ready;
use std::collections::BTreeMap;

use futures::{
	Stream,
	channel::mpsc,
	stream::{self, BoxStream, StreamExt},
};
use parking_lot::Mutex;

use super::{Error, Layer, Recoverability, recoverability};
use crate::{
	chain::HashAndNumber,
	coinage::{
		Exponent, PurseId, Snapshot,
		balance::{Balance, balance},
		classification::{ExportStatus, classify},
		ledger::{BlockRef, PurseState},
		namespace::coin_key,
		operation::{OpKey, OpKind},
		status::{StatusItem, status},
	},
	stream_extensions::StreamExtensions,
};

/// A committed change, as the event stream reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
	/// The first item of every event stream (invariant 5).
	Resynced,
	/// A purse was created.
	PurseCreated(PurseId),
	/// A purse closed.
	PurseDeleted(PurseId),
	/// The purse metadata changed: take a new backup.
	PurseMetadataChanged,
	/// An operation's status or progress changed.
	Status(OpKey, StatusItem),
	/// An exported coin's export status changed.
	ExportStatus(OpKey, [u8; 32], ExportStatus),
	/// Whether the current installation is recoverable changed.
	Recoverability(Recoverability),
	/// Observation read a new block.
	NewBlock(BlockRef),
}

/// One exported coin, as the export status stream reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExportedCoinStatus {
	/// The coin's account.
	pub account: [u8; 32],
	/// Its denomination.
	pub exponent: Exponent,
	/// What became of it.
	pub status: ExportStatus,
}

/// The rows whose change wakes every stream: this layer's records' statuses,
/// the disclosure marks, the operations and the purses.
const WATCHED: &str = "SELECT id, status, failure_kind FROM durable_tx WHERE domain IN ('coinage', 'coinage-registry')
	UNION ALL SELECT asset, committed, NULL FROM coinage_mark
	UNION ALL SELECT id, cancel_requested + 2 * released, ended FROM coinage_operation
	UNION ALL SELECT id, record, NULL FROM coinage_purse";

/// The layer's own wake signals, and the event subscribers that hear of a
/// new block.
#[derive(Default)]
pub struct Wakes {
	wakes: Mutex<Vec<mpsc::Sender<()>>>,
	blocks: Mutex<Vec<mpsc::UnboundedSender<Event>>>,
}

impl Wakes {
	/// Wakes every stream. A full channel already holds a wake, so bursts
	/// conflate; a closed one belongs to a dropped stream.
	pub fn wake(&self) {
		self.wakes.lock().retain_mut(|wake| {
			let _ = wake.try_send(());
			!wake.is_closed()
		});
	}

	/// Tells every event stream of a new block.
	pub fn new_block(&self, block: HashAndNumber) {
		let Ok(number) = u32::try_from(block.number) else { return };
		let event = Event::NewBlock(BlockRef { number, hash: block.hash.0 });
		self.blocks.lock().retain(|subscriber| subscriber.unbounded_send(event.clone()).is_ok());
	}

	fn subscribe(&self) -> mpsc::Receiver<()> {
		let (sender, receiver) = mpsc::channel(0);
		self.wakes.lock().push(sender);
		receiver
	}

	fn blocks(&self) -> mpsc::UnboundedReceiver<Event> {
		let (sender, receiver) = mpsc::unbounded();
		self.blocks.lock().push(sender);
		receiver
	}
}

impl Layer {
	/// The operation's status: the current item at once, then each change,
	/// closing after the terminal item.
	pub async fn status(&self, key: OpKey) -> Result<BoxStream<'static, StatusItem>, Error> {
		let view = self.view().await?;
		view.operation(&key).ok_or_else(|| Error::UnknownOperation(key.clone()))?;
		let items = self.reads(move |layer, view| status(view, &key, layer.now(), &layer.params()));
		Ok(distinct(items).take_until_inclusive(|item| item.status.is_terminal()))
	}

	/// The purse's balance: the current value at once, then each new value,
	/// closing when the purse closes.
	pub async fn balance(&self, purse: PurseId) -> Result<BoxStream<'static, Balance>, Error> {
		let view = self.view().await?;
		let open = move |view: &Snapshot| view.purse(purse).is_some_and(|record| !matches!(record.state, PurseState::Closed { .. }));
		if !open(&view) {
			return Err(Error::PurseNotFound(purse));
		}
		let balances = self.reads(move |layer, view| {
			Some(open(view).then(|| balance(view, purse, layer.now(), &layer.params())))
		});
		Ok(distinct(balances.take_while(|balance| ready(balance.is_some())).filter_map(ready)))
	}

	/// Every coin of an export with its export status: at once, then each
	/// change, closing once every status is final (G1 invariant 7).
	pub async fn export_status(&self, export: OpKey) -> Result<BoxStream<'static, Vec<ExportedCoinStatus>>, Error> {
		let view = self.view().await?;
		let record = view.operation(&export).ok_or_else(|| Error::UnknownOperation(export.clone()))?;
		if record.kind != OpKind::Export {
			return Err(Error::NotAnExport(export));
		}
		let entropy = self.entropy()?;
		let coins = self.reads(move |layer, view| Some(export_statuses(view, &export, &entropy, layer)));
		Ok(distinct(coins).take_until_inclusive(|coins| coins.iter().all(|coin| is_final(&coin.status))))
	}

	/// Every committed change: `Resynced` first, then each change as it is
	/// committed after the call. Never closes while the layer runs.
	pub async fn events(&self) -> Result<BoxStream<'static, Event>, Error> {
		let entropy = self.0.seed.entropy();
		let baseline = EventState::read(&self.view().await?, entropy.as_deref(), self);
		let blocks = self.0.streams.blocks();
		let states = self.reads(move |layer, view| Some(EventState::read(view, entropy.as_deref(), layer)));
		let changes = states
			.scan(baseline, |previous, state| {
				let events = previous.changes_to(&state);
				*previous = state;
				ready(Some(stream::iter(events)))
			})
			.flatten();
		Ok(stream::once(ready(Event::Resynced)).chain(stream::select(changes, blocks)).boxed())
	}

	/// `read` over a fresh snapshot at subscription and after every possible
	/// change, skipping a snapshot the store failed to give.
	fn reads<T: Send + 'static>(
		&self,
		read: impl Fn(&Layer, &Snapshot) -> Option<T> + Send + Sync + 'static,
	) -> impl Stream<Item = T> + Send + 'static {
		let watched = self.0.store.observe(WATCHED, |statement| statement.query_map([], |_| Ok(()))).map(|_| ());
		let read = std::sync::Arc::new(read);
		let layer = self.clone();
		stream::select(watched, self.0.streams.subscribe())
			.then(move |()| {
				let (layer, read) = (layer.clone(), read.clone());
				async move { layer.view().await.ok().and_then(|view| read(&layer, &view)) }
			})
			.filter_map(ready)
	}
}

/// The items without each one equal to the last one passed on.
fn distinct<T: PartialEq + Clone + Send + 'static>(items: impl Stream<Item = T> + Send + 'static) -> BoxStream<'static, T> {
	let mut last: Option<T> = None;
	items
		.filter(move |item| {
			let new = last.as_ref() != Some(item);
			if new {
				last = Some(item.clone());
			}
			ready(new)
		})
		.boxed()
}

/// Whether an export status can no longer change.
fn is_final(status: &ExportStatus) -> bool {
	matches!(
		status,
		ExportStatus::Claimed { stable: true, .. } | ExportStatus::Reclaimed { stable: true } | ExportStatus::NeverMinted
	)
}

fn export_statuses(view: &Snapshot, export: &OpKey, entropy: &[u8], layer: &Layer) -> Vec<ExportedCoinStatus> {
	let Some(record) = view.operation(export) else { return Vec::new() };
	let (now, params) = (layer.now(), layer.params());
	record
		.exported
		.iter()
		.filter_map(|(coin, exponent)| {
			let crate::coinage::Asset::Coin { purse, installation, index } = coin else { return None };
			let (_, account) = coin_key(entropy, *purse, *installation, *index).ok()?;
			let status = classify(coin, view, now, &params).export.unwrap_or(ExportStatus::NotYetMinted);
			Some(ExportedCoinStatus { account, exponent: *exponent, status })
		})
		.collect()
}

/// What the event stream compares from one snapshot to the next.
#[derive(Clone, Debug, PartialEq, Eq)]
struct EventState {
	purses: BTreeMap<PurseId, PurseState>,
	statuses: BTreeMap<OpKey, StatusItem>,
	exports: BTreeMap<(OpKey, [u8; 32]), ExportStatus>,
	recoverability: Recoverability,
}

impl EventState {
	fn read(view: &Snapshot, entropy: Option<&[u8]>, layer: &Layer) -> Self {
		let (now, params) = (layer.now(), layer.params());
		let purses = view.purses().map(|record| (record.id, record.state)).collect();
		let statuses = view
			.operations()
			.filter_map(|operation| Some((operation.key.clone(), status(view, &operation.key, now, &params)?)))
			.collect();
		let exports = match entropy {
			Some(entropy) => view
				.operations()
				.filter(|operation| operation.kind == OpKind::Export)
				.flat_map(|operation| {
					export_statuses(view, &operation.key, entropy, layer)
						.into_iter()
						.map(|coin| ((operation.key.clone(), coin.account), coin.status))
				})
				.collect(),
			None => BTreeMap::new(),
		};
		Self { purses, statuses, exports, recoverability: recoverability(view, now, &params) }
	}

	/// The events that take `self` to `next`.
	fn changes_to(&self, next: &Self) -> Vec<Event> {
		let mut events = Vec::new();
		for (purse, state) in &next.purses {
			match (self.purses.get(purse), state) {
				(None, _) => events.extend([Event::PurseCreated(*purse), Event::PurseMetadataChanged]),
				(Some(PurseState::Closed { .. }), _) | (_, PurseState::Open | PurseState::Deleting { .. }) => {},
				(Some(_), PurseState::Closed { .. }) => events.extend([Event::PurseDeleted(*purse), Event::PurseMetadataChanged]),
			}
		}
		for (key, item) in &next.statuses {
			if self.statuses.get(key) != Some(item) {
				events.push(Event::Status(key.clone(), *item));
			}
		}
		for ((key, account), status) in &next.exports {
			if self.exports.get(&(key.clone(), *account)) != Some(status) {
				events.push(Event::ExportStatus(key.clone(), *account, *status));
			}
		}
		if self.recoverability != next.recoverability {
			events.push(Event::Recoverability(next.recoverability));
		}
		events
	}
}
