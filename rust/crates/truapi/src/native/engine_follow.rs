//! Following the host's payment, top-up and balance engines.
//!
//! The core asks the host to follow an engine stream, the host pushes every
//! value that stream yields, and the core keeps what it learned and serves
//! every product stream from it. [`StatusFollower`] follows one payment or
//! top-up per id; [`BalanceFollower`] follows the user's balance.

use core::time::Duration;
use std::{
	collections::{HashMap, VecDeque},
	sync::{Arc, Mutex, MutexGuard},
};

use futures::{
	FutureExt,
	channel::mpsc,
	future::{self, AbortHandle, BoxFuture, Either},
	stream::{self, BoxStream, StreamExt},
};
use truapi::v01;

use super::errors::HostRejection;
use crate::{Spawner, platform::ProductId};

/// How long a product stream waits for an id's first status before it shows
/// the pending one.
const FIRST_STATUS_WAIT: Duration = Duration::from_millis(250);

/// How many ids' final statuses are kept, so a later subscription answers at
/// once without asking the host.
const FINALS_KEPT: usize = 512;

/// Rank of a final status, which ends every stream following its id.
const FINAL_RANK: u8 = 3;

/// What a product is told when the engine ends before reporting any status.
const STATUS_UNAVAILABLE: &str = "status is unavailable";

/// What a product is told when the host's balance stream fails or ends.
const BALANCE_UNAVAILABLE: &str = "balance is unavailable";

/// A status the host reports for one payment or top-up.
pub trait EngineStatus: Clone + Send + 'static {
	/// How a subscription that saw no status ends.
	type Error: Clone + core::fmt::Debug + Send + 'static;

	/// Shown when the engine has not answered within [`FIRST_STATUS_WAIT`].
	fn pending() -> Self;

	/// Shown when the engine's stream ends before a final status, once a
	/// product was shown a status.
	fn abandoned() -> Self;

	/// How far along the status is: a stream never steps back, and
	/// [`FINAL_RANK`] ends it.
	fn rank(&self) -> u8;

	/// Whether `error` says the engine holds no such id.
	fn is_not_found(error: &Self::Error) -> bool;

	/// Ends a subscription the engine failed before any status.
	fn unavailable() -> Self::Error;
}

impl EngineStatus for v01::HostPaymentStatusSubscribeItem {
	type Error = v01::HostPaymentStatusSubscribeError;

	fn pending() -> Self {
		Self::Processing
	}

	fn abandoned() -> Self {
		Self::Failed { reason: "payment did not complete".into() }
	}

	fn rank(&self) -> u8 {
		match self {
			Self::Processing => 0,
			Self::Completed | Self::Failed { .. } | Self::ClaimedPartially { .. } => FINAL_RANK,
		}
	}

	fn is_not_found(error: &Self::Error) -> bool {
		matches!(error, Self::Error::PaymentNotFound)
	}

	fn unavailable() -> Self::Error {
		Self::Error::Unknown { reason: STATUS_UNAVAILABLE.into() }
	}
}

impl EngineStatus for v01::HostPaymentTopUpStatusSubscribeItem {
	type Error = v01::HostPaymentTopUpStatusSubscribeError;

	fn pending() -> Self {
		Self::Detecting
	}

	fn abandoned() -> Self {
		Self::NotClaimed
	}

	fn rank(&self) -> u8 {
		match self {
			Self::Detecting => 0,
			Self::Claiming => 1,
			Self::Claimed { finalized: false } => 2,
			Self::Claimed { finalized: true } |
			Self::ClaimedPartially { .. } |
			Self::NotClaimed => FINAL_RANK,
		}
	}

	fn is_not_found(error: &Self::Error) -> bool {
		matches!(error, Self::Error::TopUpNotFound)
	}

	fn unavailable() -> Self::Error {
		Self::Error::Unknown { reason: STATUS_UNAVAILABLE.into() }
	}
}

/// Asks the host to follow one id's engine stream until it ends.
type FollowStatus<T> = Arc<
	dyn Fn(ProductId, crate::Bytes32) -> BoxFuture<'static, Result<(), <T as EngineStatus>::Error>>
		+ Send
		+ Sync,
>;

/// A payment's or top-up's id, scoped to its product.
type Key = (ProductId, crate::Bytes32);

/// What a product stream hears about the id it follows.
enum Event<T: EngineStatus> {
	Status(T),
	/// The engine's stream ended before a final status.
	Ended(T::Error),
}

/// One id the host is following.
struct Following<T: EngineStatus> {
	/// Tells this follow apart from a later one on the same id.
	follow: u64,
	latest: Option<T>,
	streams: Vec<mpsc::UnboundedSender<Event<T>>>,
	stop: AbortHandle,
}

struct Statuses<T: EngineStatus> {
	next_follow: u64,
	following: HashMap<Key, Following<T>>,
	/// The most recent [`FINALS_KEPT`] final statuses, oldest first.
	finals: VecDeque<(Key, T)>,
}

/// Follows the host's engine stream for each payment or top-up id, at most
/// once per id, and serves every product stream on that id from it.
///
/// An id is followed from the first subscription to it, or from the host
/// accepting it. A product stream shows the latest status at once; with none
/// yet it waits [`FIRST_STATUS_WAIT`] and then shows the pending one. A final
/// status ends every stream, stops the host's follow and is kept for the most
/// recent [`FINALS_KEPT`] ids. A follow that ends without one ends each stream
/// that showed a status with the abandoned one, and each other stream with the
/// engine's not-found or a fixed unavailable error; the id is then forgotten,
/// so the next subscription asks the host again.
pub struct StatusFollower<T: EngineStatus> {
	state: Arc<Mutex<Statuses<T>>>,
	follow: FollowStatus<T>,
	spawner: Spawner,
}

impl<T: EngineStatus> StatusFollower<T> {
	/// Follow ids through `follow`, run on `spawner`.
	pub fn new(follow: FollowStatus<T>, spawner: Spawner) -> Self {
		let state = Statuses { next_follow: 0, following: HashMap::new(), finals: VecDeque::new() };
		Self { state: Arc::new(Mutex::new(state)), follow, spawner }
	}

	/// Start following `product_id`'s `id` unless it is followed already or
	/// its final status is kept.
	pub fn follow(&self, product_id: ProductId, id: crate::Bytes32) {
		let key = (product_id, id);
		let mut state = lock(&self.state);
		if state.finals.iter().any(|(kept, _)| *kept == key) {
			return;
		}
		let start = self.start(&mut state, &key);
		drop(state);
		if let Some(start) = start {
			self.spawner.spawn(start);
		}
	}

	/// Stream `product_id`'s `id`: its latest status, then each later one,
	/// ending after a final one.
	pub fn subscribe(
		&self,
		product_id: ProductId,
		id: crate::Bytes32,
	) -> BoxStream<'static, Result<T, T::Error>> {
		let key = (product_id, id);
		let mut state = lock(&self.state);
		if let Some((_, status)) = state.finals.iter().find(|(kept, _)| *kept == key) {
			return stream::iter([Ok(status.clone())]).boxed();
		}
		let start = self.start(&mut state, &key);
		let (sender, events) = mpsc::unbounded();
		let following = state.following.get_mut(&key).expect("followed just above");
		if let Some(latest) = &following.latest {
			let _ = sender.unbounded_send(Event::Status(latest.clone()));
		}
		following.streams.push(sender);
		drop(state);
		if let Some(start) = start {
			self.spawner.spawn(start);
		}
		product_stream(events)
	}

	/// Take `status`, which the host pushed for `product_id`'s `id`. A status
	/// for an id nobody follows, or one behind the latest, is dropped.
	pub fn notify(&self, product_id: ProductId, id: crate::Bytes32, status: T) {
		let key = (product_id, id);
		let mut state = lock(&self.state);
		let Some(following) = state.following.get_mut(&key) else { return };
		if following.latest.as_ref().is_some_and(|latest| status.rank() <= latest.rank()) {
			return;
		}
		following.latest = Some(status.clone());
		following
			.streams
			.retain(|stream| stream.unbounded_send(Event::Status(status.clone())).is_ok());
		if status.rank() == FINAL_RANK {
			if let Some(following) = state.following.remove(&key) {
				following.stop.abort();
			}
			state.finals.push_back((key, status));
			if state.finals.len() > FINALS_KEPT {
				state.finals.pop_front();
			}
		}
	}

	/// Register a follow of `key` unless one runs, returning the task that
	/// asks the host, to spawn once the lock is released.
	fn start(&self, state: &mut Statuses<T>, key: &Key) -> Option<BoxFuture<'static, ()>> {
		if state.following.contains_key(key) {
			return None;
		}
		state.next_follow += 1;
		let follow = state.next_follow;
		let (asked, stop) = future::abortable((self.follow)(key.0.clone(), key.1));
		state
			.following
			.insert(key.clone(), Following { follow, latest: None, streams: Vec::new(), stop });
		let shared = self.state.clone();
		let key = key.clone();
		Some(
			async move {
				// Aborted once a final status arrived, which already ended
				// the follow.
				if let Ok(ended) = asked.await {
					follow_ended(&shared, &key, follow, ended);
				}
			}
			.boxed(),
		)
	}
}

/// The host's follow `follow` of `key` returned before a final status:
/// forget the id and end its streams.
fn follow_ended<T: EngineStatus>(
	state: &Mutex<Statuses<T>>,
	key: &Key,
	follow: u64,
	ended: Result<(), T::Error>,
) {
	let mut state = lock(state);
	if state.following.get(key).is_none_or(|following| following.follow != follow) {
		return;
	}
	let Some(following) = state.following.remove(key) else { return };
	drop(state);
	let error = match ended {
		Err(error) if T::is_not_found(&error) => error,
		Err(error) => {
			tracing::warn!(?error, "the host's engine status stream failed");
			T::unavailable()
		},
		Ok(()) => T::unavailable(),
	};
	for stream in following.streams {
		let _ = stream.unbounded_send(Event::Ended(error.clone()));
	}
}

/// A product's view of one id: each status once, never stepping back, with
/// the pending one if the engine is slow to answer first.
fn product_stream<T: EngineStatus>(
	events: mpsc::UnboundedReceiver<Event<T>>,
) -> BoxStream<'static, Result<T, T::Error>> {
	stream::unfold((events, None::<u8>), |(mut events, shown)| async move {
		loop {
			let event = match shown {
				Some(_) => events.next().await,
				None => {
					let first = match future::select(
						events.next(),
						futures_timer::Delay::new(FIRST_STATUS_WAIT),
					)
					.await
					{
						Either::Left((event, _)) => Some(event),
						Either::Right(_) => None,
					};
					let Some(event) = first else {
						let pending = T::pending();
						let rank = pending.rank();
						return Some((Ok(pending), (events, Some(rank))));
					};
					event
				},
			};
			match event? {
				Event::Status(status) => {
					let rank = status.rank();
					if shown.is_some_and(|shown| rank <= shown) {
						continue;
					}
					return Some((Ok(status), (events, Some(rank))));
				},
				Event::Ended(error) => {
					let last = if shown.is_some() { Ok(T::abandoned()) } else { Err(error) };
					return Some((last, (events, Some(FINAL_RANK))));
				},
			}
		}
	})
	.boxed()
}

/// Asks the host to follow the user's balance until its stream ends.
type FollowBalance = Arc<dyn Fn() -> BoxFuture<'static, Result<(), HostRejection>> + Send + Sync>;

type BalanceItem = Result<u128, v01::HostPaymentBalanceSubscribeError>;

/// The host's running balance follow.
struct BalanceFollowing {
	/// Tells this follow apart from a later one.
	follow: u64,
	latest: Option<u128>,
	streams: Vec<mpsc::UnboundedSender<BalanceItem>>,
	stop: AbortHandle,
}

struct Balance {
	next_follow: u64,
	following: Option<BalanceFollowing>,
}

/// Follows the user's balance while any product does.
///
/// The first subscription asks the host to follow its balance stream, and
/// each later one starts from the latest balance the host pushed. The core
/// stops the follow as soon as the last product stream is dropped. A follow
/// that fails or ends ends every stream with a fixed unavailable error, and
/// the next subscription asks the host again; the core never retries on its
/// own.
pub struct BalanceFollower {
	state: Arc<Mutex<Balance>>,
	follow: FollowBalance,
	spawner: Spawner,
}

impl BalanceFollower {
	/// Follow the balance through `follow`, run on `spawner`.
	pub fn new(follow: FollowBalance, spawner: Spawner) -> Self {
		let state = Balance { next_follow: 0, following: None };
		Self { state: Arc::new(Mutex::new(state)), follow, spawner }
	}

	/// Stream the balance: the latest, then each change.
	pub fn subscribe(&self) -> BoxStream<'static, BalanceItem> {
		let (sender, balances) = mpsc::unbounded();
		let mut state = lock(&self.state);
		let start = self.start(&mut state);
		let following = state.following.as_mut().expect("followed just above");
		if let Some(latest) = following.latest {
			let _ = sender.unbounded_send(Ok(latest));
		}
		following.streams.push(sender);
		drop(state);
		if let Some(start) = start {
			self.spawner.spawn(start);
		}
		BalanceStream { balances, state: self.state.clone() }.boxed()
	}

	/// Take `available`, the balance the host pushed.
	pub fn notify(&self, available: u128) {
		let mut state = lock(&self.state);
		let Some(following) = state.following.as_mut() else { return };
		if following.latest == Some(available) {
			return;
		}
		following.latest = Some(available);
		following.streams.retain(|stream| stream.unbounded_send(Ok(available)).is_ok());
		stop_if_unheard(&mut state);
	}

	fn start(&self, state: &mut Balance) -> Option<BoxFuture<'static, ()>> {
		if state.following.is_some() {
			return None;
		}
		state.next_follow += 1;
		let follow = state.next_follow;
		let (asked, stop) = future::abortable((self.follow)());
		state.following =
			Some(BalanceFollowing { follow, latest: None, streams: Vec::new(), stop });
		let shared = self.state.clone();
		Some(
			async move {
				// Aborted once nobody listened, which already ended the
				// follow.
				let Ok(ended) = asked.await else { return };
				if let Err(error) = ended {
					tracing::warn!(%error, "the host's balance stream failed");
				}
				let mut state = lock(&shared);
				if state.following.as_ref().is_none_or(|following| following.follow != follow) {
					return;
				}
				let Some(following) = state.following.take() else { return };
				drop(state);
				let unavailable = v01::HostPaymentBalanceSubscribeError::Unknown {
					reason: BALANCE_UNAVAILABLE.into(),
				};
				for stream in following.streams {
					let _ = stream.unbounded_send(Err(unavailable.clone()));
				}
			}
			.boxed(),
		)
	}
}

/// Stop the host's balance follow once no product stream listens to it.
fn stop_if_unheard(state: &mut Balance) {
	if state.following.as_ref().is_some_and(|following| following.streams.is_empty()) &&
		let Some(following) = state.following.take()
	{
		following.stop.abort();
	}
}

/// A product's balance stream, which stops the host's follow when it is the
/// last one dropped.
struct BalanceStream {
	balances: mpsc::UnboundedReceiver<BalanceItem>,
	state: Arc<Mutex<Balance>>,
}

impl futures::Stream for BalanceStream {
	type Item = BalanceItem;

	fn poll_next(
		mut self: core::pin::Pin<&mut Self>,
		cx: &mut core::task::Context<'_>,
	) -> core::task::Poll<Option<BalanceItem>> {
		self.balances.poll_next_unpin(cx)
	}
}

impl Drop for BalanceStream {
	fn drop(&mut self) {
		// Closed first, so this stream's sender reads as closed below.
		self.balances.close();
		let mut state = lock(&self.state);
		if let Some(following) = state.following.as_mut() {
			following.streams.retain(|stream| !stream.is_closed());
		}
		stop_if_unheard(&mut state);
	}
}

fn lock<S>(state: &Mutex<S>) -> MutexGuard<'_, S> {
	state.lock().expect("engine follower mutex poisoned")
}

#[cfg(test)]
mod tests;
