//! The RFC-6 engine the payment capability serves products from: payments,
//! top-ups and the balance view. The coinage layers implement all three
//! (`CoinageEngine`, layer 2 N2) and install them once they start, through
//! [`super::RuntimeServices`]; until then, and where coinage does not run,
//! every RFC-6 call answers `Unsupported`.

use futures::stream::BoxStream;

use crate::{
	Bytes32,
	latest::{
		HostPaymentBalanceSubscribeError, HostPaymentBalanceSubscribeItem, HostPaymentError, HostPaymentRequest,
		HostPaymentStatusSubscribeError, HostPaymentStatusSubscribeItem, HostPaymentTopUpError,
		HostPaymentTopUpRequest, HostPaymentTopUpStatusSubscribeError, HostPaymentTopUpStatusSubscribeItem,
	},
	platform::ProductContext,
};

/// What a payment request can spend right now, per purse. The balance is the
/// user's, not the product's: every product following a purse sees the same
/// figure. The payment capability asks for the product's balance access
/// before calling here, and checks it again before passing on each balance
/// the stream emits, so a revoked grant ends the stream with
/// `PermissionDenied`.
pub trait BalanceView: Send + Sync {
	/// Emit the balance of `purse` (`None` for the main purse) now and on
	/// every change.
	fn subscribe_balance(
		&self,
		product: &ProductContext,
		purse: Option<u32>,
	) -> BoxStream<'static, Result<HostPaymentBalanceSubscribeItem, HostPaymentBalanceSubscribeError>>;
}

/// Claims a source's funds into the user's balance. The payment capability
/// validates the source keys and hashes the product into the id before
/// calling, so ids never collide across products.
#[async_trait::async_trait]
pub trait TopUpEngine: Send + Sync {
	/// Start a top-up. Returns once the engine has accepted it.
	async fn top_up(
		&self,
		product: &ProductContext,
		request: HostPaymentTopUpRequest,
	) -> Result<(), HostPaymentTopUpError>;

	/// Emit a top-up's current status and every later one, ending after a
	/// terminal status.
	fn subscribe_top_up_status(
		&self,
		product: &ProductContext,
		id: Bytes32,
	) -> BoxStream<'static, Result<HostPaymentTopUpStatusSubscribeItem, HostPaymentTopUpStatusSubscribeError>>;
}

/// Pays from the user's balance to an account, once the user approves. The
/// payment capability hashes the product into the id before calling, so ids
/// never collide across products.
#[async_trait::async_trait]
pub trait PaymentEngine: Send + Sync {
	/// Ask the user to approve `request`. Returns once the user has decided:
	/// `Ok` when they authorized it and the engine took it on; the payment's
	/// outcome arrives through its status.
	async fn request_payment(&self, product: &ProductContext, request: HostPaymentRequest)
	-> Result<(), HostPaymentError>;

	/// Emit a payment's current status and every later one, ending after a
	/// terminal status.
	fn subscribe_payment_status(
		&self,
		product: &ProductContext,
		id: Bytes32,
	) -> BoxStream<'static, Result<HostPaymentStatusSubscribeItem, HostPaymentStatusSubscribeError>>;
}
