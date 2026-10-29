//! Unified [`NftPurse`] trait (NFT purses RFC).

use crate::{
	CallContext, CallError, Subscription,
	versioned::nft_purse::{
		HostNftPurseCreateChequeError, HostNftPurseCreateChequeRequest,
		HostNftPurseCreateChequeResponse, HostNftPurseCreateItemsError,
		HostNftPurseCreateItemsRequest, HostNftPurseCreateItemsResponse,
		HostNftPurseCreatePurseError, HostNftPurseCreatePurseRequest,
		HostNftPurseCreatePurseResponse, HostNftPurseCreateReceiveKeyError,
		HostNftPurseCreateReceiveKeyRequest, HostNftPurseCreateReceiveKeyResponse,
		HostNftPurseDepositError, HostNftPurseDepositItem, HostNftPurseDepositRequest,
		HostNftPurseListPursesError, HostNftPurseListPursesRequest, HostNftPurseListPursesResponse,
		HostNftPurseListenForPaymentError, HostNftPurseListenForPaymentItem,
		HostNftPurseListenForPaymentRequest, HostNftPurseQueryPurseError,
		HostNftPurseQueryPurseRequest, HostNftPurseQueryPurseResponse,
		HostNftPurseRebalancePurseError, HostNftPurseRebalancePurseItem,
		HostNftPurseRebalancePurseRequest, HostNftPurseRefundError, HostNftPurseRefundItem,
		HostNftPurseRefundRequest, HostNftPurseRevokeChequeError, HostNftPurseRevokeChequeRequest,
		HostNftPurseRevokeChequeResponse, HostNftPurseTransferItemError,
		HostNftPurseTransferItemItem, HostNftPurseTransferItemRequest,
	},
	wire, wire_trait,
};

/// Purses of NFT items, each item held by its own host-derived key.
///
/// A product uses the purses it created without any check. Reading a purse
/// owned by Root or another product needs the `NftPurseRead` permission for
/// that owner; draining one asks the user to confirm each call. Long-running
/// operations are subscriptions whose items are the RFC status updates.
#[wire_trait(id = 26)]
#[crate::async_trait]
pub trait NftPurse: Send + Sync {
	/// Create a purse owned by the calling product.
	///
	/// The name is at most 64 bytes of UTF-8. A longer name fails with a
	/// generic error carrying its length.
	///
	/// ```ts
	/// const result = await truapi.nftPurse.createPurse({ name: "Tickets" });
	/// assert(result.isOk(), "createPurse failed:", result);
	/// console.log("purse created:", result.value.purse);
	/// ```
	#[wire(id = 0)]
	async fn create_purse(
		&self,
		_cx: &CallContext,
		_request: HostNftPurseCreatePurseRequest,
	) -> Result<HostNftPurseCreatePurseResponse, CallError<HostNftPurseCreatePurseError>> {
		Err(CallError::unavailable())
	}

	/// Create new item keys in a purse.
	///
	/// ```ts
	/// const result = await truapi.nftPurse.createItems({ purse: 0, count: 1 });
	/// assert(result.isOk(), "createItems failed:", result);
	/// console.log("item keys:", result.value.items);
	/// ```
	#[wire(id = 1)]
	async fn create_items(
		&self,
		_cx: &CallContext,
		_request: HostNftPurseCreateItemsRequest,
	) -> Result<HostNftPurseCreateItemsResponse, CallError<HostNftPurseCreateItemsError>> {
		Err(CallError::unavailable())
	}

	/// Read one page of the items in a purse that are on chain and not in use.
	///
	/// ```ts
	/// const result = await truapi.nftPurse.queryPurse({ purse: 0, page: 0, pageSize: 20 });
	/// assert(result.isOk(), "queryPurse failed:", result);
	/// console.log("purse page:", result.value.page);
	/// ```
	#[wire(id = 2)]
	async fn query_purse(
		&self,
		_cx: &CallContext,
		_request: HostNftPurseQueryPurseRequest,
	) -> Result<HostNftPurseQueryPurseResponse, CallError<HostNftPurseQueryPurseError>> {
		Err(CallError::unavailable())
	}

	/// List the purses of an owner, by id.
	///
	/// The caller's own purses need no permission. Root's purses, or another
	/// product's, need the `NftPurseRead` permission for that owner.
	///
	/// ```ts
	/// const productContext = await truapi.system.getProductContext();
	/// assert(productContext.isOk(), "getProductContext failed:", productContext);
	/// const result = await truapi.nftPurse.listPurses({
	///   owner: { tag: "Product", value: productContext.value.productId },
	/// });
	/// assert(result.isOk(), "listPurses failed:", result);
	/// console.log("purses:", result.value.purses);
	/// ```
	#[wire(id = 3)]
	async fn list_purses(
		&self,
		_cx: &CallContext,
		_request: HostNftPurseListPursesRequest,
	) -> Result<HostNftPurseListPursesResponse, CallError<HostNftPurseListPursesError>> {
		Err(CallError::unavailable())
	}

	/// Move items from one purse to another.
	///
	/// ```ts
	/// import { firstValueFrom, from } from "rxjs";
	///
	/// const status = await firstValueFrom(
	///   from(
	///     truapi.nftPurse.rebalancePurse({
	///       request: {
	///         from: 0,
	///         to: 1,
	///         keys: ["0x0000000000000000000000000000000000000000000000000000000000000000"],
	///       },
	///     }),
	///   ),
	/// );
	/// console.log("rebalance status:", status);
	/// ```
	#[wire(id = 4)]
	async fn rebalance_purse(
		&self,
		_cx: &CallContext,
		_request: HostNftPurseRebalancePurseRequest,
	) -> Subscription<HostNftPurseRebalancePurseItem, CallError<HostNftPurseRebalancePurseError>> {
		Subscription::interrupted(CallError::unavailable())
	}

	/// Create a one-time receive key for a payment into a purse.
	///
	/// ```ts
	/// const result = await truapi.nftPurse.createReceiveKey({ into: 0 });
	/// assert(result.isOk(), "createReceiveKey failed:", result);
	/// console.log("receive key:", result.value.key);
	/// ```
	#[wire(id = 5)]
	async fn create_receive_key(
		&self,
		_cx: &CallContext,
		_request: HostNftPurseCreateReceiveKeyRequest,
	) -> Result<HostNftPurseCreateReceiveKeyResponse, CallError<HostNftPurseCreateReceiveKeyError>>
	{
		Err(CallError::unavailable())
	}

	/// Listen for the cheque sent to a receive key.
	///
	/// ```ts
	/// import { firstValueFrom, from } from "rxjs";
	///
	/// const item = await firstValueFrom(
	///   from(
	///     truapi.nftPurse.listenForPayment({
	///       request: {
	///         key: "0x0000000000000000000000000000000000000000000000000000000000000000",
	///       },
	///     }),
	///   ),
	/// );
	/// console.log("payment item:", item);
	/// ```
	#[wire(id = 6)]
	async fn listen_for_payment(
		&self,
		_cx: &CallContext,
		_request: HostNftPurseListenForPaymentRequest,
	) -> Subscription<HostNftPurseListenForPaymentItem, CallError<HostNftPurseListenForPaymentError>>
	{
		Subscription::interrupted(CallError::unavailable())
	}

	/// Hand items off in a cheque that the product delivers.
	///
	/// ```ts
	/// const result = await truapi.nftPurse.createCheque({
	///   from: 0,
	///   to: {
	///     tag: "Ephemeral",
	///     value: "0x0000000000000000000000000000000000000000000000000000000000000000",
	///   },
	///   items: ["0x0000000000000000000000000000000000000000000000000000000000000000"],
	/// });
	/// assert(result.isOk(), "createCheque failed:", result);
	/// console.log("cheque:", result.value.cheque);
	/// ```
	#[wire(id = 7)]
	async fn create_cheque(
		&self,
		_cx: &CallContext,
		_request: HostNftPurseCreateChequeRequest,
	) -> Result<HostNftPurseCreateChequeResponse, CallError<HostNftPurseCreateChequeError>> {
		Err(CallError::unavailable())
	}

	/// Release the items of a cheque the recipient never received.
	///
	/// ```ts
	/// const result = await truapi.nftPurse.revokeCheque({
	///   chequeId: "0x0000000000000000000000000000000000000000000000000000000000000000",
	/// });
	/// assert(result.isOk(), "revokeCheque failed:", result);
	/// ```
	#[wire(id = 8)]
	async fn revoke_cheque(
		&self,
		_cx: &CallContext,
		_request: HostNftPurseRevokeChequeRequest,
	) -> Result<HostNftPurseRevokeChequeResponse, CallError<HostNftPurseRevokeChequeError>> {
		Err(CallError::unavailable())
	}

	/// Deposit a received cheque into a purse.
	///
	/// ```ts
	/// import { type NftPurseCheque } from "@parity/truapi";
	/// import { firstValueFrom, from } from "rxjs";
	///
	/// const cheque: NftPurseCheque = {
	///   version: 0,
	///   id: "0x0000000000000000000000000000000000000000000000000000000000000000",
	///   recipient: { tag: "Static" },
	///   items: ["0x0000000000000000000000000000000000000000000000000000000000000000"],
	///   encryptedSecrets: "0x",
	/// };
	///
	/// const status = await firstValueFrom(
	///   from(truapi.nftPurse.deposit({ request: { cheque, to: 0 } })),
	/// );
	/// console.log("deposit status:", status);
	/// ```
	#[wire(id = 9)]
	async fn deposit(
		&self,
		_cx: &CallContext,
		_request: HostNftPurseDepositRequest,
	) -> Subscription<HostNftPurseDepositItem, CallError<HostNftPurseDepositError>> {
		Subscription::interrupted(CallError::unavailable())
	}

	/// Return the items of a deposited cheque to their original keys.
	///
	/// ```ts
	/// import { firstValueFrom, from } from "rxjs";
	///
	/// const status = await firstValueFrom(
	///   from(
	///     truapi.nftPurse.refund({
	///       request: {
	///         chequeId: "0x0000000000000000000000000000000000000000000000000000000000000000",
	///       },
	///     }),
	///   ),
	/// );
	/// console.log("refund status:", status);
	/// ```
	#[wire(id = 10)]
	async fn refund(
		&self,
		_cx: &CallContext,
		_request: HostNftPurseRefundRequest,
	) -> Subscription<HostNftPurseRefundItem, CallError<HostNftPurseRefundError>> {
		Subscription::interrupted(CallError::unavailable())
	}

	/// Move one item to a key outside the purses.
	///
	/// ```ts
	/// import { firstValueFrom, from } from "rxjs";
	///
	/// const status = await firstValueFrom(
	///   from(
	///     truapi.nftPurse.transferItem({
	///       request: {
	///         item: "0x0000000000000000000000000000000000000000000000000000000000000000",
	///         destination: "0x0000000000000000000000000000000000000000000000000000000000000000",
	///       },
	///     }),
	///   ),
	/// );
	/// console.log("transfer status:", status);
	/// ```
	#[wire(id = 11)]
	async fn transfer_item(
		&self,
		_cx: &CallContext,
		_request: HostNftPurseTransferItemRequest,
	) -> Subscription<HostNftPurseTransferItemItem, CallError<HostNftPurseTransferItemError>> {
		Subscription::interrupted(CallError::unavailable())
	}
}
