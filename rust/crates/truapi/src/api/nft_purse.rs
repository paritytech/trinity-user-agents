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
		HostNftPurseDepositError, HostNftPurseDepositRequest, HostNftPurseDepositResponse,
		HostNftPurseListPursesError, HostNftPurseListPursesRequest, HostNftPurseListPursesResponse,
		HostNftPurseListenForPaymentError, HostNftPurseListenForPaymentItem,
		HostNftPurseListenForPaymentRequest, HostNftPurseObserveOperationStatusError,
		HostNftPurseObserveOperationStatusItem, HostNftPurseObserveOperationStatusRequest,
		HostNftPurseQueryPurseError, HostNftPurseQueryPurseRequest, HostNftPurseQueryPurseResponse,
		HostNftPurseRebalancePurseError, HostNftPurseRebalancePurseRequest,
		HostNftPurseRebalancePurseResponse, HostNftPurseRefundError, HostNftPurseRefundRequest,
		HostNftPurseRefundResponse, HostNftPurseRevokeChequeError, HostNftPurseRevokeChequeRequest,
		HostNftPurseRevokeChequeResponse, HostNftPurseStaticReceiveKeyError,
		HostNftPurseStaticReceiveKeyRequest, HostNftPurseStaticReceiveKeyResponse,
		HostNftPurseTransferItemError, HostNftPurseTransferItemRequest,
		HostNftPurseTransferItemResponse,
	},
	wire, wire_trait,
};

/// Purses of NFT items, each item held by its own host-derived key.
///
/// A product uses the purses it created without any check. Reading a purse
/// owned by Root or another product needs the `NftPurseRead` permission for
/// that owner; draining one asks the user to confirm each call. A chain
/// operation returns once it is registered under the product's idempotency
/// key, and `observe_operation_status` follows it.
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

	/// Transfer items from one purse to fresh keys of another. Returns once
	/// the transfers are registered under the idempotency key.
	///
	/// ```ts
	/// const result = await truapi.nftPurse.rebalancePurse({
	///   from: 0,
	///   to: 1,
	///   keys: ["0x0000000000000000000000000000000000000000000000000000000000000000"],
	///   idempotencyKey: "0x0101010101010101010101010101010101010101010101010101010101010101",
	/// });
	/// assert(result.isOk(), "rebalancePurse failed:", result);
	/// ```
	#[wire(id = 4)]
	async fn rebalance_purse(
		&self,
		_cx: &CallContext,
		_request: HostNftPurseRebalancePurseRequest,
	) -> Result<HostNftPurseRebalancePurseResponse, CallError<HostNftPurseRebalancePurseError>> {
		Err(CallError::unavailable())
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

	/// Hand items off in a cheque that the product delivers. The items are
	/// spent from then on. The cheque id is the destination's: derived from
	/// the receive key, or the given static `cheque_id`. A product creating a
	/// second cheque with the same id gets `OperationExists`.
	/// `encrypted_secrets` is the ephemeral X25519 key, then the 12-byte nonce,
	/// then the ChaCha20-Poly1305 ciphertext of the SCALE-encoded 64-byte
	/// sr25519 secrets in `items` order, each as schnorrkel's
	/// `SecretKey::to_bytes` writes it (scalar, then nonce), not the
	/// ed25519-expanded form.
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

	/// Move the items of a cheque the calling product created back into fresh
	/// keys of the purse they left, so its secrets no longer reach them.
	/// Returns once the moves are registered. Their status is observed under
	/// the idempotency key `blake2_256("nft-purse/revoke" ‖ chequeId)`.
	/// Revoking again retries only the keys whose move failed.
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

	/// Deposit a received cheque into a purse. Returns once the deposit is
	/// registered under the cheque id.
	///
	/// ```ts
	/// import { type NftPurseCheque } from "@parity/truapi";
	///
	/// const cheque: NftPurseCheque = {
	///   version: 0,
	///   id: "0x0000000000000000000000000000000000000000000000000000000000000000",
	///   recipient: { tag: "Static" },
	///   items: ["0x0000000000000000000000000000000000000000000000000000000000000000"],
	///   encryptedSecrets: "0x",
	/// };
	///
	/// const result = await truapi.nftPurse.deposit({ cheque, to: 0 });
	/// assert(result.isOk(), "deposit failed:", result);
	/// ```
	#[wire(id = 9)]
	async fn deposit(
		&self,
		_cx: &CallContext,
		_request: HostNftPurseDepositRequest,
	) -> Result<HostNftPurseDepositResponse, CallError<HostNftPurseDepositError>> {
		Err(CallError::unavailable())
	}

	/// Return the items of a cheque the calling product deposited to the
	/// cheque's keys, once every move of the deposit finalized or failed.
	/// Items spent since stay and are listed as unavailable. Asks the user to
	/// confirm when the product does not own the purse. Returns once the moves
	/// are registered. Their status is observed under the idempotency key
	/// `blake2_256("nft-purse/refund" ‖ chequeId)`.
	///
	/// ```ts
	/// const result = await truapi.nftPurse.refund({
	///   chequeId: "0x0000000000000000000000000000000000000000000000000000000000000000",
	/// });
	/// assert(result.isOk(), "refund failed:", result);
	/// console.log("returning:", result.value.started);
	/// ```
	#[wire(id = 10)]
	async fn refund(
		&self,
		_cx: &CallContext,
		_request: HostNftPurseRefundRequest,
	) -> Result<HostNftPurseRefundResponse, CallError<HostNftPurseRefundError>> {
		Err(CallError::unavailable())
	}

	/// Transfer one item to a key outside the purses. Returns once the transfer is
	/// registered under the idempotency key.
	///
	/// ```ts
	/// const result = await truapi.nftPurse.transferItem({
	///   item: "0x0000000000000000000000000000000000000000000000000000000000000000",
	///   destination: "0x0000000000000000000000000000000000000000000000000000000000000000",
	///   idempotencyKey: "0x0202020202020202020202020202020202020202020202020202020202020202",
	/// });
	/// assert(result.isOk(), "transferItem failed:", result);
	/// ```
	#[wire(id = 11)]
	async fn transfer_item(
		&self,
		_cx: &CallContext,
		_request: HostNftPurseTransferItemRequest,
	) -> Result<HostNftPurseTransferItemResponse, CallError<HostNftPurseTransferItemError>> {
		Err(CallError::unavailable())
	}

	/// Follow an operation the calling product registered: a rebalance or
	/// transfer by its idempotency key, a deposit by its cheque id, and a
	/// revoke or refund by the key `revokeCheque` or `refund` gives. Ends
	/// after a terminal status.
	///
	/// ```ts
	/// import { firstValueFrom, from } from "rxjs";
	///
	/// const status = await firstValueFrom(
	///   from(
	///     truapi.nftPurse.observeOperationStatus({
	///       request: {
	///         idempotencyKey: "0x0101010101010101010101010101010101010101010101010101010101010101",
	///       },
	///     }),
	///   ),
	/// );
	/// console.log("operation status:", status);
	/// ```
	#[wire(id = 12)]
	async fn observe_operation_status(
		&self,
		_cx: &CallContext,
		_request: HostNftPurseObserveOperationStatusRequest,
	) -> Subscription<
		HostNftPurseObserveOperationStatusItem,
		CallError<HostNftPurseObserveOperationStatusError>,
	> {
		Subscription::interrupted(CallError::unavailable())
	}

	/// The X25519 key a sender seals a `Static` cheque to, so the product can
	/// publish it. The same for every product.
	///
	/// ```ts
	/// const result = await truapi.nftPurse.staticReceiveKey({});
	/// assert(result.isOk(), "staticReceiveKey failed:", result);
	/// console.log("static receive key:", result.value.key);
	/// ```
	#[wire(id = 13)]
	async fn static_receive_key(
		&self,
		_cx: &CallContext,
		_request: HostNftPurseStaticReceiveKeyRequest,
	) -> Result<HostNftPurseStaticReceiveKeyResponse, CallError<HostNftPurseStaticReceiveKeyError>>
	{
		Err(CallError::unavailable())
	}
}
