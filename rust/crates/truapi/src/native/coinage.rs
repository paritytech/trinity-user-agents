//! The coinage native interface (`docs/design/coinage/native-interface.md`):
//! the handle a native host's wallet screens and chat use layer 1 through,
//! and the observers they follow its streams with.
//!
//! [`NativeTrUApiHostRuntime::coinage`](super::NativeTrUApiHostRuntime::coinage)
//! hands out a [`NativeCoinage`] once the layer has started. Every operation
//! it starts is keyed in the app's scope, on the main purse.

use std::sync::Arc;

use futures::{StreamExt, stream::BoxStream};

use super::renderer::NativeSubscription;
use crate::{
	Spawner,
	coinage::{
		CoinSecret, Initiator, PurseId,
		policy::PrivacyPreset,
		assembly::Funding,
		balance::{Balance, Holdings},
		handover::{HandOverProgress, app_key},
		layer::{Error, Event, Export, ExportedCoinStatus, Layer, Op, Recoverability, Started},
		operation::Retry,
		reconstruction::Purses,
		status::{Completion, StatusItem},
	},
};

/// Follows a purse's balance.
#[uniffi::export(foreign)]
pub trait NativeBalanceObserver: Send + Sync {
	/// The balance now, and after each change.
	fn on_balance(&self, balance: Balance);
}

/// Follows a purse's holdings.
#[uniffi::export(foreign)]
pub trait NativeHoldingsObserver: Send + Sync {
	/// The holdings now, and after each change.
	fn on_holdings(&self, holdings: Holdings);
}

/// Follows one operation's status.
#[uniffi::export(foreign)]
pub trait NativeStatusObserver: Send + Sync {
	/// The status now, and after each change, ending with the terminal one.
	fn on_status(&self, item: StatusItem);
}

/// Follows what became of an export's coins.
#[uniffi::export(foreign)]
pub trait NativeExportStatusObserver: Send + Sync {
	/// Every coin's export status now, and after each change.
	fn on_export_status(&self, coins: Vec<ExportedCoinStatus>);
}

/// Follows the layer's events.
#[uniffi::export(foreign)]
pub trait NativeEventObserver: Send + Sync {
	/// One committed change, `Resynced` first.
	fn on_event(&self, event: Event);
}

/// Follows the scan of a wallet the app handed over.
#[uniffi::export(foreign)]
pub trait NativeHandOverObserver: Send + Sync {
	/// The progress now, and after each change.
	fn on_progress(&self, progress: HandOverProgress);
}

/// The account a top-up is paid from, and its signer.
#[uniffi::export(foreign)]
pub trait NativeFunding: Send + Sync {
	/// The account that pays.
	fn account(&self) -> [u8; 32];
	/// An sr25519 signature over `payload`, or none when the user declines.
	fn sign(&self, payload: Vec<u8>) -> Option<[u8; 64]>;
}

/// A native funding capability as layer 1's.
struct FundingBridge(Arc<dyn NativeFunding>);

impl Funding for FundingBridge {
	fn account(&self) -> [u8; 32] {
		self.0.account()
	}

	fn sign(&self, payload: &[u8]) -> Option<[u8; 64]> {
		self.0.sign(payload.to_vec())
	}
}

/// The coinage layer as a native host's screens use it: every call on the
/// main purse, every operation keyed in the app's scope.
#[derive(uniffi::Object)]
pub struct NativeCoinage {
	layer: Layer,
	spawner: Spawner,
}

impl NativeCoinage {
	/// A handle on `layer`, delivering to observers on `spawner`.
	pub fn new(layer: Layer, spawner: Spawner) -> Self {
		Self { layer, spawner }
	}

	/// Delivers every item of `stream` to `deliver` until the stream ends or
	/// the subscription is cancelled.
	fn follow<T: Send + 'static>(
		&self,
		mut stream: BoxStream<'static, T>,
		deliver: impl Fn(T) + Send + 'static,
	) -> Arc<NativeSubscription> {
		self.spawner.spawn_observation(async move {
			while let Some(item) = stream.next().await {
				deliver(item);
			}
		})
	}
}

/// The app's operation `id`, retried as `retry` says.
fn app_op(id: [u8; 32], retry: Retry) -> Op {
	Op { key: app_key(id), retry }
}

#[uniffi::export]
impl NativeCoinage {
	/// The main purse's balance now.
	pub async fn balance(&self) -> Result<Balance, Error> {
		Ok(self.layer.query_purse(PurseId::MAIN).await?.balance)
	}

	/// Follows the main purse's balance.
	pub async fn observe_balance(&self, observer: Arc<dyn NativeBalanceObserver>) -> Result<Arc<NativeSubscription>, Error> {
		let stream = self.layer.balance(PurseId::MAIN).await?;
		Ok(self.follow(stream, move |balance| observer.on_balance(balance)))
	}

	/// Follows the main purse's holdings.
	pub async fn observe_holdings(
		&self,
		observer: Arc<dyn NativeHoldingsObserver>,
	) -> Result<Arc<NativeSubscription>, Error> {
		let stream = self.layer.holdings(PurseId::MAIN).await?;
		Ok(self.follow(stream, move |holdings| observer.on_holdings(holdings)))
	}

	/// Whether the installation can be found from the seed alone.
	pub async fn recoverable(&self) -> Result<Recoverability, Error> {
		self.layer.recoverable().await
	}

	/// Follows the layer's events.
	pub async fn observe_events(&self, observer: Arc<dyn NativeEventObserver>) -> Result<Arc<NativeSubscription>, Error> {
		let stream = self.layer.events().await?;
		Ok(self.follow(stream, move |event| observer.on_event(event)))
	}

	/// The external asset's amount in one cent.
	pub fn asset_unit(&self) -> u128 {
		self.layer.asset_unit()
	}

	/// Hands coins worth `amount` cents to a payee, with their secrets,
	/// uncommitted until [`Self::commit_export`].
	pub async fn export(&self, id: [u8; 32], retry: Retry, amount: u64, confirmed: bool) -> Result<Export, Error> {
		self.layer.export(app_op(id, retry), PurseId::MAIN, amount, Initiator::User, confirmed).await
	}

	/// Commits the export `id`; the app calls it before it saves the message
	/// that carries the secrets.
	pub async fn commit_export(&self, id: [u8; 32]) -> Result<(), Error> {
		self.layer.commit_export(app_key(id)).await
	}

	/// Gives up the export `id`, which was never committed.
	pub async fn release_export(&self, id: [u8; 32]) -> Result<(), Error> {
		self.layer.release_export(app_key(id)).await
	}

	/// Takes back, under `id`, the coins of the committed export `export`.
	pub async fn reclaim(&self, id: [u8; 32], retry: Retry, export: [u8; 32]) -> Result<Started, Error> {
		self.layer.reclaim(app_op(id, retry), app_key(export), PurseId::MAIN).await
	}

	/// Claims `coins` into the main purse under `id`.
	pub async fn import(&self, id: [u8; 32], retry: Retry, coins: Vec<CoinSecret>) -> Result<Started, Error> {
		self.layer.import(app_op(id, retry), PurseId::MAIN, coins).await
	}

	/// Whether a claim without a stable verdict holds one of `coins`.
	pub async fn importing(&self, coins: Vec<CoinSecret>) -> Result<bool, Error> {
		self.layer.importing(&coins).await
	}

	/// Loads `amount` cents into the main purse, paid from `funding`.
	pub async fn top_up(
		&self,
		id: [u8; 32],
		retry: Retry,
		amount: u64,
		funding: Arc<dyn NativeFunding>,
	) -> Result<Started, Error> {
		self.layer.top_up(app_op(id, retry), PurseId::MAIN, amount, Arc::new(FundingBridge(funding))).await
	}

	/// Moves `amount` cents out of the main purse to the external account
	/// `to`.
	pub async fn offload(
		&self,
		id: [u8; 32],
		retry: Retry,
		amount: u64,
		to: [u8; 32],
		confirmed: bool,
	) -> Result<Started, Error> {
		self.layer.offload(app_op(id, retry), PurseId::MAIN, amount, to, Initiator::User, confirmed).await
	}

	/// Follows the status of the app's operation `id`.
	pub async fn observe_status(
		&self,
		id: [u8; 32],
		observer: Arc<dyn NativeStatusObserver>,
	) -> Result<Arc<NativeSubscription>, Error> {
		let stream = self.layer.status(app_key(id)).await?;
		Ok(self.follow(stream, move |item| observer.on_status(item)))
	}

	/// Follows what became of the coins of the export `id`.
	pub async fn observe_export_status(
		&self,
		id: [u8; 32],
		observer: Arc<dyn NativeExportStatusObserver>,
	) -> Result<Arc<NativeSubscription>, Error> {
		let stream = self.layer.export_status(app_key(id)).await?;
		Ok(self.follow(stream, move |coins| observer.on_export_status(coins)))
	}

	/// The completion of the app's operation `id`, once it has ended.
	pub async fn completion(&self, id: [u8; 32]) -> Result<Option<Completion>, Error> {
		self.layer.completion(app_key(id)).await
	}

	/// Asks to cancel the app's operation `id`.
	pub async fn cancel(&self, id: [u8; 32]) -> Result<(), Error> {
		self.layer.cancel(app_key(id)).await
	}

	/// Reconstructs the main purse from the seed, further than the default
	/// scan when `deep`.
	pub async fn reconstruct(&self, id: [u8; 32], deep: bool) -> Result<Started, Error> {
		self.layer.reconstruct(app_op(id, Retry::Never), Purses::MainOnly, deep).await
	}

	/// Applies the privacy setting the user chose. The app calls it after
	/// each start with its stored choice, and whenever the user changes it;
	/// it re-classifies and migrates nothing (layer 1, A4).
	pub async fn set_privacy(&self, preset: PrivacyPreset) -> Result<(), Error> {
		let checked = self.layer.set_policy(preset.policy()).await?;
		checked.expect("every preset passes the policy check");
		Ok(())
	}

	/// How far the scan of a wallet the app handed over has come.
	pub async fn hand_over_progress(&self) -> Result<HandOverProgress, Error> {
		self.layer.hand_over_progress().await
	}

	/// Follows the scan of a wallet the app handed over.
	pub fn observe_hand_over(&self, observer: Arc<dyn NativeHandOverObserver>) -> Arc<NativeSubscription> {
		self.follow(self.layer.hand_over(), move |progress| observer.on_progress(progress))
	}
}
