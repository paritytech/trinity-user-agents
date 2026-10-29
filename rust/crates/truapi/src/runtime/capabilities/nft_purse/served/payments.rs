//! Listening for NFT payments: the channel a product shares with a sender,
//! then the cheques that arrive on it.

use futures::{StreamExt, stream};
use truapi::{CallError, Subscription, latest};

use super::host_failure;
use crate::{
	nft_purse::{PaymentListen, PaymentListenError},
	runtime::ProductRuntimeHost,
};

impl ProductRuntimeHost {
	/// The channel of the caller's receive key, then the first cheque sent to
	/// it. A listen that did not start emits no channel.
	pub async fn listen_one_time_nft_payment(
		&self,
		request: latest::HostNftPurseListenOneTimePaymentRequest,
	) -> Subscription<
		latest::HostNftPurseListenOneTimePaymentItem,
		CallError<latest::HostNftPurseListenOneTimePaymentError>,
	> {
		self.start_one_time_nft_payment_listen(request)
			.await
			.unwrap_or_else(Subscription::interrupted)
	}

	async fn start_one_time_nft_payment_listen(
		&self,
		request: latest::HostNftPurseListenOneTimePaymentRequest,
	) -> Result<
		Subscription<
			latest::HostNftPurseListenOneTimePaymentItem,
			CallError<latest::HostNftPurseListenOneTimePaymentError>,
		>,
		CallError<latest::HostNftPurseListenOneTimePaymentError>,
	> {
		self.require_session()?;
		let purses = self.connection.nft_purses()?;
		let listen = purses
			.start_one_time_payment_listen(self.connection.product_id(), request.key)
			.await
			.map_err(one_time_payment_failure)?;
		Ok(channel_then_arrivals(
			listen,
			latest::HostNftPurseListenOneTimePaymentItem::Channel,
			latest::HostNftPurseListenOneTimePaymentItem::Cheque,
			one_time_payment_failure,
		))
	}

	/// The channel of the caller's inbox `inbox_domain`, then the cheques
	/// already there, then each batch of new ones. A listen that did not
	/// start emits no channel.
	pub async fn listen_nft_payment_inbox(
		&self,
		request: latest::HostNftPurseListenPaymentInboxRequest,
	) -> Subscription<
		latest::HostNftPurseListenPaymentInboxItem,
		CallError<latest::HostNftPurseListenPaymentInboxError>,
	> {
		self.start_nft_payment_inbox_listen(request)
			.await
			.unwrap_or_else(Subscription::interrupted)
	}

	async fn start_nft_payment_inbox_listen(
		&self,
		request: latest::HostNftPurseListenPaymentInboxRequest,
	) -> Result<
		Subscription<
			latest::HostNftPurseListenPaymentInboxItem,
			CallError<latest::HostNftPurseListenPaymentInboxError>,
		>,
		CallError<latest::HostNftPurseListenPaymentInboxError>,
	> {
		self.require_session()?;
		let purses = self.connection.nft_purses()?;
		let account = self.identity_account().ok_or_else(no_identity_account)?;
		let caller = self.connection.product_id();
		let listen = purses
			.start_payment_inbox_listen(account, caller, request.inbox_domain)
			.await
			.map_err(payment_inbox_failure)?;
		Ok(channel_then_arrivals(
			listen,
			latest::HostNftPurseListenPaymentInboxItem::Channel,
			latest::HostNftPurseListenPaymentInboxItem::Cheques,
			payment_inbox_failure,
		))
	}

	/// The wallet identity account of the session, which inbox topics name.
	fn identity_account(&self) -> Option<[u8; 32]> {
		self.accounts.current_session()?.identity_account_id
	}
}

/// The channel of `listen`, then each of its arrivals, each wrapped in the
/// call's item, and its final error mapped by `failure`.
fn channel_then_arrivals<T: Send + 'static, I: Send + 'static, E: Send + 'static>(
	listen: PaymentListen<T>,
	channel: fn(latest::NftPursePaymentTransmissionChannel) -> I,
	arrival: fn(T) -> I,
	failure: fn(PaymentListenError) -> CallError<E>,
) -> Subscription<I, CallError<E>> {
	let standard =
		latest::NftPursePaymentTransmissionChannel::Standard { sss_topic: listen.statement_topic };
	let arrivals = listen.arrivals.map(move |arrived| arrived.map(arrival).map_err(failure));
	let items = stream::once(async move { Ok(channel(standard)) }).chain(arrivals);
	Subscription::new(items)
}

/// An unknown or foreign receive key is the product's error, a database
/// failure the host's, and anything else ends the listen with `Other`.
fn one_time_payment_failure(
	error: PaymentListenError,
) -> CallError<latest::HostNftPurseListenOneTimePaymentError> {
	match error {
		PaymentListenError::UnknownReceiveKey(_) => {
			CallError::Domain(latest::HostNftPurseListenOneTimePaymentError::ReceiveKeyNotFound)
		},
		PaymentListenError::Db(error) => host_failure(error),
		error => {
			other_refusal(error.to_string(), latest::HostNftPurseListenOneTimePaymentError::Other)
		},
	}
}

/// A database failure is the host's, and anything else ends the listen with
/// `Other`.
fn payment_inbox_failure(
	error: PaymentListenError,
) -> CallError<latest::HostNftPurseListenPaymentInboxError> {
	match error {
		PaymentListenError::Db(error) => host_failure(error),
		error => {
			other_refusal(error.to_string(), latest::HostNftPurseListenPaymentInboxError::Other)
		},
	}
}

fn no_identity_account() -> CallError<latest::HostNftPurseListenPaymentInboxError> {
	let reason = "the session has no wallet identity account to name the inbox".to_owned();
	other_refusal(reason, latest::HostNftPurseListenPaymentInboxError::Other)
}

/// The product-facing `Other` error of a call, built by `other`, for `reason`.
fn other_refusal<E>(reason: String, other: fn(latest::GenericError) -> E) -> CallError<E> {
	CallError::Domain(other(latest::GenericError { reason }))
}
