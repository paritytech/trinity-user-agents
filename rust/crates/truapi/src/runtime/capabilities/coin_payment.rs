//! N1 · RFC-17 surface: the `CoinPayment` wire trait, served from the
//! coinage layers.
//!
//! Every call is authorized through J1 before anything else happens: the
//! permissions it lists are asked for once and stored by the core's
//! permission service, and every spend is confirmed by the user (N1
//! invariant 1). A product never sees a coin secret outside a sealed cheque
//! (N1 invariant 2). A host that runs no coinage layer answers every call as
//! unsupported.

use futures::{
	StreamExt,
	stream::{self, BoxStream},
};
use tracing::instrument;
use truapi::{
	CallContext, CallError, Subscription,
	api::CoinPayment,
	latest::{self, CoinPaymentError, CoinPaymentStatus},
	versioned::coin_payment::{
		HostCoinPaymentCancelChequeError, HostCoinPaymentCancelChequeItem, HostCoinPaymentCancelChequeRequest,
		HostCoinPaymentChequeStatusError, HostCoinPaymentChequeStatusItem, HostCoinPaymentChequeStatusRequest,
		HostCoinPaymentCreateChequeError, HostCoinPaymentCreateChequeRequest, HostCoinPaymentCreateChequeResponse,
		HostCoinPaymentCreatePurseError, HostCoinPaymentCreatePurseRequest, HostCoinPaymentCreatePurseResponse,
		HostCoinPaymentCreateReceivableError, HostCoinPaymentCreateReceivableRequest,
		HostCoinPaymentCreateReceivableResponse, HostCoinPaymentDeletePurseError, HostCoinPaymentDeletePurseItem,
		HostCoinPaymentDeletePurseRequest, HostCoinPaymentDepositError, HostCoinPaymentDepositItem,
		HostCoinPaymentDepositRequest, HostCoinPaymentListPursesError, HostCoinPaymentListPursesRequest,
		HostCoinPaymentListPursesResponse, HostCoinPaymentListenForError, HostCoinPaymentListenForItem,
		HostCoinPaymentListenForRequest, HostCoinPaymentQueryPurseError, HostCoinPaymentQueryPurseRequest,
		HostCoinPaymentQueryPurseResponse, HostCoinPaymentRebalancePurseError, HostCoinPaymentRebalancePurseItem,
		HostCoinPaymentRebalancePurseRequest, HostCoinPaymentRefundError, HostCoinPaymentRefundItem,
		HostCoinPaymentRefundRequest,
	},
};

use crate::{
	coinage::{
		Initiator, PurseId,
		layer::{Op, PurseInfo},
		operation::{OpKey, Retry},
		payments::{
			cheque::cheque_hash,
			flows::{PayError, Payments},
			permissions::{Action, Requirement, Surface, requirements},
			receivable::Receivable,
			records,
			status::{ChequeProgress, coin_payment_error, own_move_status},
			topic::payment_topic,
		},
	},
	platform::{CoinPaymentSpendReview, UserConfirmationReview},
	runtime::{AccountHolder, AuthorityError, ProductRuntimeHost, versioning::version},
	stream_extensions::StreamExtensions,
};

#[async_trait::async_trait]
impl<H: AccountHolder> CoinPayment for ProductRuntimeHost<H> {
	#[instrument(skip_all, fields(runtime.method = "coin_payment.create_purse"))]
	async fn create_purse(
		&self,
		cx: &CallContext,
		request: HostCoinPaymentCreatePurseRequest,
	) -> Result<HostCoinPaymentCreatePurseResponse, CallError<HostCoinPaymentCreatePurseError>> {
		version(request)
			.exec(|latest::HostCoinPaymentCreatePurseRequest { name }| async move {
				let payments = self.coinage().await?;
				self.authorize(cx, PurseId::MAIN, None, Action::TopUp, 0, false).await?;
				let tag = self.connection.product_id().as_str().as_bytes().to_vec();
				let purse = payments.layer().create_purse(name, tag).await.map_err(layer_failure)?;
				Ok(latest::HostCoinPaymentCreatePurseResponse { purse: purse.0 })
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "coin_payment.query_purse"))]
	async fn query_purse(
		&self,
		cx: &CallContext,
		request: HostCoinPaymentQueryPurseRequest,
	) -> Result<HostCoinPaymentQueryPurseResponse, CallError<HostCoinPaymentQueryPurseError>> {
		version(request)
			.exec(|latest::HostCoinPaymentQueryPurseRequest { purse }| async move {
				let payments = self.coinage().await?;
				let info = self.purse(&payments, PurseId(purse)).await?;
				self.authorize(cx, info.id, Some(&info.tag), Action::Query, 0, false).await?;
				Ok(latest::HostCoinPaymentQueryPurseResponse { info: purse_info(&info) })
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "coin_payment.rebalance_purse"))]
	async fn rebalance_purse(
		&self,
		cx: &CallContext,
		request: HostCoinPaymentRebalancePurseRequest,
	) -> Subscription<HostCoinPaymentRebalancePurseItem, CallError<HostCoinPaymentRebalancePurseError>> {
		version(request)
			.exec(|latest::HostCoinPaymentRebalancePurseRequest { from, to, amount }| async move {
				let started = async {
					let payments = self.coinage().await?;
					let source = self.purse(&payments, PurseId(from)).await?;
					let destination = self.purse(&payments, PurseId(to)).await?;
					self.authorize(cx, source.id, Some(&source.tag), Action::Spend, amount, false).await?;
					self.authorize(cx, destination.id, Some(&destination.tag), Action::Receive, 0, false).await?;
					let op = Op { key: payments.random_key(), retry: Retry::ExpiryOnly };
					let key = op.key.clone();
					let layer = payments.layer();
					layer.rebalance(op, source.id, destination.id, u64::from(amount)).await.map_err(layer_failure)?;
					own_move(&payments, key).await
				};
				subscription(started.await)
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "coin_payment.delete_purse"))]
	async fn delete_purse(
		&self,
		cx: &CallContext,
		request: HostCoinPaymentDeletePurseRequest,
	) -> Subscription<HostCoinPaymentDeletePurseItem, CallError<HostCoinPaymentDeletePurseError>> {
		version(request)
			.exec(|latest::HostCoinPaymentDeletePurseRequest { target, drain_into }| async move {
				let started = async {
					let payments = self.coinage().await?;
					let purse = self.purse(&payments, PurseId(target)).await?;
					let drain = self.purse(&payments, PurseId(drain_into)).await?;
					if purse.tag != self.connection.product_id().as_str().as_bytes() {
						return Err(CallError::Domain(CoinPaymentError::Denied));
					}
					let amount = cents(purse.balance.spendable + purse.balance.held_back + purse.balance.arriving);
					self.authorize(cx, purse.id, Some(&purse.tag), Action::Spend, amount, false).await?;
					self.authorize(cx, drain.id, Some(&drain.tag), Action::Receive, 0, false).await?;
					let op = Op { key: payments.random_key(), retry: Retry::ExpiryOnly };
					let key = op.key.clone();
					payments.layer().delete_purse(op, purse.id, drain.id).await.map_err(layer_failure)?;
					own_move(&payments, key).await
				};
				subscription(started.await)
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "coin_payment.create_receivable"))]
	async fn create_receivable(
		&self,
		cx: &CallContext,
		request: HostCoinPaymentCreateReceivableRequest,
	) -> Result<HostCoinPaymentCreateReceivableResponse, CallError<HostCoinPaymentCreateReceivableError>> {
		version(request)
			.exec(|latest::HostCoinPaymentCreateReceivableRequest { into }| async move {
				let payments = self.coinage().await?;
				let purse = self.purse(&payments, PurseId(into)).await?;
				self.authorize(cx, purse.id, Some(&purse.tag), Action::Receive, 0, false).await?;
				let receivable = payments.create_receivable(purse.id).await.map_err(internal)?;
				Ok(latest::HostCoinPaymentCreateReceivableResponse { receivable: receivable.0 })
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "coin_payment.create_cheque"))]
	async fn create_cheque(
		&self,
		cx: &CallContext,
		request: HostCoinPaymentCreateChequeRequest,
	) -> Result<HostCoinPaymentCreateChequeResponse, CallError<HostCoinPaymentCreateChequeError>> {
		version(request)
			.exec(|latest::HostCoinPaymentCreateChequeRequest { from, to, amount }| async move {
				let payments = self.coinage().await?;
				let source = self.purse(&payments, PurseId(from)).await?;
				self.authorize(cx, source.id, Some(&source.tag), Action::Spend, amount, false).await?;
				let product = self.connection.product_id().as_str().to_owned();
				let to = Receivable(to);
				let paid = payments.pay(&product, source.id, to, amount, Initiator::Product, false).await;
				let cheque = match paid {
					Err(PayError::NeedsConfirmation) => {
						self.confirm_spend(cx, source.id, amount, true).await?;
						payments.pay(&product, source.id, to, amount, Initiator::Product, true).await
					},
					paid => paid,
				};
				let cheque = cheque.map_err(|error| match error {
					PayError::Refused(error) => CallError::Domain(error),
					PayError::NeedsConfirmation => CallError::Domain(CoinPaymentError::Denied),
				})?;
				Ok(latest::HostCoinPaymentCreateChequeResponse { cheque })
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "coin_payment.deposit"))]
	async fn deposit(
		&self,
		cx: &CallContext,
		request: HostCoinPaymentDepositRequest,
	) -> Subscription<HostCoinPaymentDepositItem, CallError<HostCoinPaymentDepositError>> {
		version(request)
			.exec(|latest::HostCoinPaymentDepositRequest { cheque }| async move {
				let started = async {
					let payments = self.coinage().await?;
					let purse = self.receivable_purse(&payments, Receivable(cheque.id)).await?;
					self.authorize(cx, purse.id, Some(&purse.tag), Action::Receive, 0, true).await?;
					let product = self.connection.product_id().as_str().to_owned();
					payments.deposit(&product, cheque).await.map_err(CallError::Domain)
				};
				subscription(started.await)
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "coin_payment.refund"))]
	async fn refund(
		&self,
		cx: &CallContext,
		request: HostCoinPaymentRefundRequest,
	) -> Subscription<HostCoinPaymentRefundItem, CallError<HostCoinPaymentRefundError>> {
		version(request)
			.exec(|latest::HostCoinPaymentRefundRequest { receivable }| async move {
				let started = async {
					let payments = self.coinage().await?;
					let receivable = Receivable(receivable);
					let purse = self.receivable_purse(&payments, receivable).await?;
					self.authorize(cx, purse.id, Some(&purse.tag), Action::Spend, 0, true).await?;
					let product = self.connection.product_id().as_str().to_owned();
					payments.refund(&product, receivable).await.map_err(CallError::Domain)
				};
				subscription(started.await)
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "coin_payment.listen_for_payment"))]
	async fn listen_for_payment(
		&self,
		cx: &CallContext,
		request: HostCoinPaymentListenForRequest,
	) -> Subscription<HostCoinPaymentListenForItem, CallError<HostCoinPaymentListenForError>> {
		version(request)
			.exec(|latest::HostCoinPaymentListenForRequest { receivable }| async move {
				let started = async {
					let payments = self.coinage().await?;
					let receivable = Receivable(receivable);
					let purse = self.receivable_purse(&payments, receivable).await?;
					self.authorize(cx, purse.id, Some(&purse.tag), Action::Receive, 0, true).await?;
					let channel = latest::CoinPaymentTransmissionChannel::Standard {
						sss_topic: payment_topic(receivable).0,
					};
					let cheques = payments.listen(receivable).await.map_err(CallError::Domain)?;
					let first = latest::HostCoinPaymentListenForItem::Channel(channel);
					Ok(stream::once(core::future::ready(first))
						.chain(cheques.map(latest::HostCoinPaymentListenForItem::Cheque))
						.boxed())
				};
				subscription(started.await)
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "coin_payment.cheque_status"))]
	async fn cheque_status(
		&self,
		cx: &CallContext,
		request: HostCoinPaymentChequeStatusRequest,
	) -> Subscription<HostCoinPaymentChequeStatusItem, CallError<HostCoinPaymentChequeStatusError>> {
		version(request)
			.exec(|latest::HostCoinPaymentChequeStatusRequest { cheque }| async move {
				let started = async {
					let payments = self.coinage().await?;
					let hash = cheque_hash(&cheque);
					let source = self.cheque_source(&payments, hash).await?;
					self.authorize(cx, source.id, Some(&source.tag), Action::Query, 0, true).await?;
					let progress = payments.cheque_status(hash).await.map_err(CallError::Domain)?;
					Ok(progress.map(|progress| cheque_progress(&progress)).boxed())
				};
				subscription(started.await)
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "coin_payment.cancel_cheque"))]
	async fn cancel_cheque(
		&self,
		cx: &CallContext,
		request: HostCoinPaymentCancelChequeRequest,
	) -> Subscription<HostCoinPaymentCancelChequeItem, CallError<HostCoinPaymentCancelChequeError>> {
		version(request)
			.exec(|latest::HostCoinPaymentCancelChequeRequest { cheque }| async move {
				let started = async {
					let payments = self.coinage().await?;
					let hash = cheque_hash(&cheque);
					let source = self.cheque_source(&payments, hash).await?;
					self.authorize(cx, source.id, Some(&source.tag), Action::Spend, cheque.amount, true).await?;
					payments.cancel(hash).await.map_err(CallError::Domain)
				};
				subscription(started.await)
			})
			.await
	}

	#[instrument(skip_all, fields(runtime.method = "coin_payment.list_purses"))]
	async fn list_purses(
		&self,
		_cx: &CallContext,
		request: HostCoinPaymentListPursesRequest,
	) -> Result<HostCoinPaymentListPursesResponse, CallError<HostCoinPaymentListPursesError>> {
		version(request)
			.exec(|latest::HostCoinPaymentListPursesRequest {}| async move {
				let payments = self.coinage().await?;
				let product = self.connection.product_id().as_str().as_bytes().to_vec();
				let mut purses = Vec::new();
				for info in payments.layer().list_purses().await.map_err(layer_failure)? {
					if self.may_see(&product, &info).await {
						purses.push(latest::CoinPaymentListedPurse { purse: info.id.0, info: purse_info(&info) });
					}
				}
				Ok(latest::HostCoinPaymentListPursesResponse { purses })
			})
			.await
	}
}

impl<H: AccountHolder> ProductRuntimeHost<H> {
	/// The coinage payments of this host, after the session check;
	/// `Unsupported` where no coinage layer runs. Waits while the layers start.
	async fn coinage<E>(&self) -> Result<Payments, CallError<E>> {
		let payments = self.connection.services.coinage_payments().await.ok_or(CallError::Unsupported)?;
		self.require_session()?;
		Ok(payments)
	}

	/// Asks for everything J1 lists for `action` on `purse`, created by
	/// `creator`: the stored permissions, then a spend's confirmation of
	/// `amount` cents. A refusal is `Denied`, or `ReceivableNotFound` where
	/// the call names a receivable rather than a purse (N1 invariant 3).
	async fn authorize(
		&self,
		cx: &CallContext,
		purse: PurseId,
		creator: Option<&[u8]>,
		action: Action,
		amount: u32,
		names_a_receivable: bool,
	) -> Result<(), CallError<CoinPaymentError>> {
		let product = self.connection.product_id().as_str().as_bytes().to_vec();
		let denied =
			if names_a_receivable { CoinPaymentError::ReceivableNotFound } else { CoinPaymentError::Denied };
		for requirement in requirements(&product, purse, creator, action, Surface::Rfc17) {
			match requirement {
				Requirement::Permission(permission) => {
					self.connection.require_remote_permission(permission, denied.clone()).await?
				},
				Requirement::ConfirmSpend => self.confirm_spend(cx, purse, amount, false).await?,
			}
		}
		Ok(())
	}

	/// Asks the user to confirm spending `amount` cents from `purse`.
	async fn confirm_spend(
		&self,
		cx: &CallContext,
		purse: PurseId,
		amount: u32,
		takes_held_back_value: bool,
	) -> Result<(), CallError<CoinPaymentError>> {
		let unit = self.coinage::<CoinPaymentError>().await?.layer().asset_unit();
		let review = UserConfirmationReview::CoinPaymentSpend(CoinPaymentSpendReview {
			calling_product_id: self.connection.product_id().clone(),
			from_purse: purse.0,
			amount: u128::from(amount).saturating_mul(unit),
			takes_held_back_value,
		});
		match self.accounts.confirm(cx, review).await {
			Ok(()) => Ok(()),
			Err(AuthorityError::Rejected) => Err(CallError::Domain(CoinPaymentError::Denied)),
			Err(error) => Err(CallError::HostFailure { reason: error.to_string() }),
		}
	}

	/// A purse that is not closed, or `PurseNotFound`.
	async fn purse(&self, payments: &Payments, purse: PurseId) -> Result<PurseInfo, CallError<CoinPaymentError>> {
		payments.layer().query_purse(purse).await.map_err(layer_failure)
	}

	/// The purse a receivable this host holds deposits into, resolved
	/// through any deletion; `ReceivableNotFound` for any other receivable.
	async fn receivable_purse(
		&self,
		payments: &Payments,
		receivable: Receivable,
	) -> Result<PurseInfo, CallError<CoinPaymentError>> {
		let record = payments.store().read(move |conn| records::receivable(conn, receivable)).await.map_err(internal)?;
		let record = record.ok_or(CallError::Domain(CoinPaymentError::ReceivableNotFound))?;
		let purse = payments.layer().resolve_purse(record.purse).await.map_err(layer_failure)?;
		self.purse(payments, purse).await
	}

	/// The purse one of this host's outgoing cheques was paid from, resolved
	/// through any deletion; `ReceivableNotFound` for any other cheque.
	async fn cheque_source(
		&self,
		payments: &Payments,
		cheque: crate::coinage::payments::cheque::ChequeHash,
	) -> Result<PurseInfo, CallError<CoinPaymentError>> {
		let outgoing = payments.store().read(move |conn| records::outgoing(conn, cheque)).await.map_err(internal)?;
		let outgoing = outgoing.ok_or(CallError::Domain(CoinPaymentError::ReceivableNotFound))?;
		let purse = payments.layer().resolve_purse(outgoing.source).await.map_err(layer_failure)?;
		self.purse(payments, purse).await
	}

	/// Whether the product may see `purse` without asking anyone (N1
	/// invariant 4): it created it, or holds the permissions to query it.
	async fn may_see(&self, product: &[u8], purse: &PurseInfo) -> bool {
		for requirement in requirements(product, purse.id, Some(&purse.tag), Action::Query, Surface::Rfc17) {
			let Requirement::Permission(permission) = requirement else { return false };
			if !self.connection.holds_remote_permission(permission).await {
				return false;
			}
		}
		true
	}
}

/// A rebalance's or a deletion's status stream, mapped as H5 says, closing
/// after its terminal item.
async fn own_move(
	payments: &Payments,
	key: OpKey,
) -> Result<BoxStream<'static, CoinPaymentStatus>, CallError<CoinPaymentError>> {
	let items = payments.layer().status(key).await.map_err(layer_failure)?;
	Ok(items
		.map(own_move_status)
		.take_until_inclusive(|status| !matches!(status, CoinPaymentStatus::Clearing { .. }))
		.boxed())
}

/// A started stream as a subscription, or the failure that kept it from
/// starting.
fn subscription<T: Send + 'static>(
	started: Result<BoxStream<'static, T>, CallError<CoinPaymentError>>,
) -> Subscription<T, CallError<CoinPaymentError>> {
	match started {
		Ok(items) => Subscription::new(items.map(Ok)),
		Err(error) => Subscription::interrupted(error),
	}
}

fn purse_info(info: &PurseInfo) -> latest::CoinPaymentPurseInfo {
	latest::CoinPaymentPurseInfo {
		name: info.name.clone(),
		created: info.created,
		creator: String::from_utf8_lossy(&info.tag).into_owned(),
		balance: u32::try_from(info.balance.spendable).unwrap_or(u32::MAX),
	}
}

fn cheque_progress(progress: &ChequeProgress) -> latest::CoinPaymentChequeProgress {
	let narrow = |amount: u64| u32::try_from(amount).unwrap_or(u32::MAX);
	latest::CoinPaymentChequeProgress {
		minting: narrow(progress.minting),
		claimable: narrow(progress.claimable),
		claimed: narrow(progress.claimed),
		returned: narrow(progress.returned),
	}
}

fn cents(amount: u64) -> u32 {
	u32::try_from(amount).unwrap_or(u32::MAX)
}

fn layer_failure(error: crate::coinage::layer::Error) -> CallError<CoinPaymentError> {
	CallError::Domain(coin_payment_error(&error))
}

fn internal<E>(_: E) -> CallError<CoinPaymentError> {
	CallError::Domain(CoinPaymentError::Internal)
}
