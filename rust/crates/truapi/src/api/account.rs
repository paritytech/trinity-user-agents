//! Unified [`Account`] trait.

use crate::versioned::account::{
    HostAccountConnectionStatusSubscribeError, HostAccountConnectionStatusSubscribeItem,
    HostAccountConnectionStatusSubscribeRequest, HostAccountCreateProofError,
    HostAccountCreateProofRequest, HostAccountCreateProofResponse, HostAccountGetAliasError,
    HostAccountGetAliasRequest, HostAccountGetAliasResponse, HostAccountGetError,
    HostAccountGetRequest, HostAccountGetResponse, HostAccountListRingVrfKeysError,
    HostAccountListRingVrfKeysRequest, HostAccountListRingVrfKeysResponse,
    HostAccountRegisterRingVrfKeyError, HostAccountRegisterRingVrfKeyRequest,
    HostAccountRegisterRingVrfKeyResponse, HostAccountRingVrfSignError,
    HostAccountRingVrfSignRequest, HostAccountRingVrfSignResponse, HostAccountSignVrfError,
    HostAccountSignVrfRequest, HostAccountSignVrfResponse, HostGetLegacyAccountsError,
    HostGetLegacyAccountsRequest, HostGetLegacyAccountsResponse, HostGetUserIdError,
    HostGetUserIdRequest, HostGetUserIdResponse, HostProductDeviceChatError,
    HostProductDeviceChatRequest, HostProductDeviceChatResponse, HostRequestLoginError,
    HostRequestLoginRequest, HostRequestLoginResponse,
};
use crate::{CallContext, CallError, Subscription};
use crate::{wire, wire_trait};

/// Account lookup, aliasing, and proof generation.
#[wire_trait(id = 2)]
#[crate::async_trait]
pub trait Account: Send + Sync {
    /// Subscribe to account connection status changes.
    ///
    /// ```ts
    /// import { firstValueFrom, from } from "rxjs";
    ///
    /// const status = await firstValueFrom(
    ///   from(truapi.account.connectionStatusSubscribe()),
    /// );
    /// console.log("connection status:", status);
    /// ```
    #[wire(id = 0)]
    async fn connection_status_subscribe(
        &self,
        _cx: &CallContext,
        _request: HostAccountConnectionStatusSubscribeRequest,
    ) -> Subscription<
        HostAccountConnectionStatusSubscribeItem,
        CallError<HostAccountConnectionStatusSubscribeError>,
    > {
        Subscription::interrupted(CallError::unavailable())
    }

    /// Retrieve a product-scoped account.
    ///
    /// ```ts
    /// const productContext = await truapi.system.getProductContext();
    /// assert(productContext.isOk(), "getProductContext failed:", productContext);
    ///
    /// const result = await truapi.account.getAccount({
    ///   productAccountId: {
    ///     dotNsIdentifier: productContext.value.productId,
    ///     derivationIndex: { tag: "Index", value: 0 },
    ///   },
    /// });
    /// assert(result.isOk(), "getAccount failed:", result);
    /// console.log("account retrieved:", result.value);
    ///
    /// const otherProduct = await truapi.account.getAccount({
    ///   productAccountId: {
    ///     dotNsIdentifier: "other-product.dot",
    ///     derivationIndex: { tag: "Index", value: 0 },
    ///   },
    /// });
    /// assert(otherProduct.isOk(), "cross-product getAccount was denied or failed:", otherProduct);
    /// console.log("other product account retrieved after approval:", otherProduct.value);
    /// ```
    #[wire(id = 1)]
    async fn get_account(
        &self,
        _cx: &CallContext,
        _request: HostAccountGetRequest,
    ) -> Result<HostAccountGetResponse, CallError<HostAccountGetError>> {
        Err(CallError::unavailable())
    }

    /// Retrieve the contextual alias for a context and ring.
    ///
    /// ```ts
    /// const productContext = await truapi.system.getProductContext();
    /// assert(productContext.isOk(), "getProductContext failed:", productContext);
    ///
    /// const people = await truapi.chain.getChainInfo({ chain: "People" });
    /// assert(people.isOk(), "getChainInfo failed:", people);
    ///
    /// const PEOPLE_COLLECTION_ID =
    ///   "0x706f703a706f6c6b61646f742e6e6574776f726b2f70656f706c652d6c697465" as const;
    /// const keyHandle = {
    ///   dotNsIdentifier: productContext.value.productId,
    ///   derivationIndex: { tag: "Index" as const, value: 0 },
    /// };
    /// const ringLocation = {
    ///   chainId: people.value.genesisHash,
    ///   junctions: [
    ///     { tag: "CollectionId" as const, value: PEOPLE_COLLECTION_ID },
    ///   ],
    /// };
    /// const registration = await truapi.account.registerRingVrfKey({
    ///   index: keyHandle.derivationIndex,
    ///   ring: ringLocation,
    /// });
    /// assert(registration.isOk(), "registerRingVrfKey failed:", registration);
    ///
    /// const result = await truapi.account.getAccountAlias({
    ///   keyHandle,
    ///   context: { productId: productContext.value.productId, suffix: { tag: "Index", value: 0 } },
    ///   ringLocation,
    /// });
    /// assert(result.isOk(), "getAccountAlias failed:", result);
    /// console.log("account alias:", result.value);
    /// ```
    #[wire(id = 2)]
    async fn get_account_alias(
        &self,
        _cx: &CallContext,
        _request: HostAccountGetAliasRequest,
    ) -> Result<HostAccountGetAliasResponse, CallError<HostAccountGetAliasError>> {
        Err(CallError::unavailable())
    }

    /// Generate a ring VRF proof with an explicitly registered member key.
    ///
    /// ```ts
    /// const productContext = await truapi.system.getProductContext();
    /// assert(productContext.isOk(), "getProductContext failed:", productContext);
    ///
    /// const people = await truapi.chain.getChainInfo({ chain: "People" });
    /// assert(people.isOk(), "getChainInfo failed:", people);
    ///
    /// const PEOPLE_COLLECTION_ID =
    ///   "0x706f703a706f6c6b61646f742e6e6574776f726b2f70656f706c652d6c697465";
    ///
    /// const result = await truapi.account.createAccountProof({
    ///   keyHandle: {
    ///     dotNsIdentifier: "peopl.dot",
    ///     derivationIndex: { tag: "Index", value: 1 },
    ///   },
    ///   context: { productId: productContext.value.productId, suffix: { tag: "Index", value: 0 } },
    ///   ringLocation: {
    ///     chainId: people.value.genesisHash,
    ///     junctions: [
    ///       { tag: "CollectionId", value: PEOPLE_COLLECTION_ID },
    ///     ],
    ///   },
    ///   message: "0x48656c6c6f",
    /// });
    /// assert(result.isErr(), "foreign createAccountProof unexpectedly succeeded:", result);
    /// // RFC-0024 forbids a prompt fallback for a bearer proof made with a
    /// // foreign key, so the only correct outcome is a refusal. Which refusal
    /// // depends on how far the call gets: the session is consulted before the
    /// // grant, so a signed-out host answers `Rejected` and a signed-in one
    /// // without a `context` grant answers `NotAllowlisted`. Both are refusals
    /// // and neither asks the user anything, which is what this demonstrates.
    /// const refusal =
    ///   result.error.tag === "Domain" && result.error.value.tag === "V1"
    ///     ? result.error.value.value.tag
    ///     : result.error.tag;
    /// assert(
    ///   refusal === "NotAllowlisted" || refusal === "Rejected",
    ///   "foreign createAccountProof was not refused:",
    ///   result,
    /// );
    /// console.log(`foreign account proof refused without prompting: ${refusal}`);
    /// ```
    #[wire(id = 3)]
    async fn create_account_proof(
        &self,
        _cx: &CallContext,
        _request: HostAccountCreateProofRequest,
    ) -> Result<HostAccountCreateProofResponse, CallError<HostAccountCreateProofError>> {
        Err(CallError::unavailable())
    }

    /// Produce an sr25519 (schnorrkel) VRF signature from a product account.
    ///
    /// The host builds a Merlin transcript from `transcriptLabel` and `items`
    /// and signs it with the account's key, returning the VRF pre-output and
    /// proof. Authorized like signing: local when `AutoSigning` covers the
    /// account, otherwise a per-call user confirmation.
    ///
    /// ```ts
    /// const productContext = await truapi.system.getProductContext();
    /// assert(productContext.isOk(), "getProductContext failed:", productContext);
    ///
    /// const result = await truapi.account.signVrf({
    ///   account: {
    ///     dotNsIdentifier: productContext.value.productId,
    ///     derivationIndex: { tag: "Index", value: 0 },
    ///   },
    ///   transcriptLabel: "0x706f703a61697264726f70",
    ///   items: [
    ///     { label: "0x646f6d61696e", value: "0x706f703a61697264726f70" },
    ///     { label: "0x7369676e6572", value: "0x00" },
    ///   ],
    /// });
    /// assert(result.isOk(), "signVrf failed:", result);
    /// console.log("vrf signature:", result.value);
    /// ```
    #[wire(id = 7)]
    async fn sign_vrf(
        &self,
        _cx: &CallContext,
        _request: HostAccountSignVrfRequest,
    ) -> Result<HostAccountSignVrfResponse, CallError<HostAccountSignVrfError>> {
        Err(CallError::unavailable())
    }

    /// Register a ring-VRF key owned by the calling product.
    ///
    /// ```ts
    /// const people = await truapi.chain.getChainInfo({ chain: "People" });
    /// assert(people.isOk(), "getChainInfo failed:", people);
    ///
    /// const PEOPLE_COLLECTION_ID =
    ///   "0x706f703a706f6c6b61646f742e6e6574776f726b2f70656f706c652d6c697465";
    ///
    /// const result = await truapi.account.registerRingVrfKey({
    ///   index: { tag: "Index", value: 0 },
    ///   ring: {
    ///     chainId: people.value.genesisHash,
    ///     junctions: [
    ///       { tag: "CollectionId", value: PEOPLE_COLLECTION_ID },
    ///     ],
    ///   },
    /// });
    /// assert(result.isOk(), "registerRingVrfKey failed:", result);
    /// console.log("ring VRF public key:", result.value);
    /// ```
    #[wire(id = 8)]
    async fn register_ring_vrf_key(
        &self,
        _cx: &CallContext,
        _request: HostAccountRegisterRingVrfKeyRequest,
    ) -> Result<HostAccountRegisterRingVrfKeyResponse, CallError<HostAccountRegisterRingVrfKeyError>>
    {
        Err(CallError::unavailable())
    }

    /// List registered ring-VRF keys owned by a product.
    ///
    /// ```ts
    /// const result = await truapi.account.listRingVrfKeys({
    ///   owner: "peopl.paseo",
    ///   disclosure: "Anonymized",
    /// });
    /// assert(result.isOk(), "listRingVrfKeys failed:", result);
    /// assert(
    ///   [0, 1].every((index) => result.value.some(({ handle }) =>
    ///     handle.dotNsIdentifier === "peopl.paseo"
    ///     && handle.derivationIndex.tag === "Index"
    ///     && handle.derivationIndex.value === index,
    ///   )),
    ///   "Expected built-in full and lite personhood keys:",
    ///   result.value,
    /// );
    /// console.log("registered ring VRF keys:", result.value);
    /// ```
    #[wire(id = 9)]
    async fn list_ring_vrf_keys(
        &self,
        _cx: &CallContext,
        _request: HostAccountListRingVrfKeysRequest,
    ) -> Result<HostAccountListRingVrfKeysResponse, CallError<HostAccountListRingVrfKeysError>>
    {
        Err(CallError::unavailable())
    }

    /// Sign bytes directly with a registered ring-VRF member key.
    ///
    /// ```ts
    /// const productContext = await truapi.system.getProductContext();
    /// assert(productContext.isOk(), "getProductContext failed:", productContext);
    ///
    /// const result = await truapi.account.ringVrfSign({
    ///   keyHandle: {
    ///     dotNsIdentifier: productContext.value.productId,
    ///     derivationIndex: { tag: "Index", value: 0 },
    ///   },
    ///   message: "0x48656c6c6f",
    /// });
    /// assert(result.isOk(), "ringVrfSign failed:", result);
    /// console.log("ring VRF signature:", result.value);
    /// ```
    #[wire(id = 10)]
    async fn ring_vrf_sign(
        &self,
        _cx: &CallContext,
        _request: HostAccountRingVrfSignRequest,
    ) -> Result<HostAccountRingVrfSignResponse, CallError<HostAccountRingVrfSignError>> {
        Err(CallError::unavailable())
    }

    /// Use a non-exportable Host Chat device for native cryptographic operations
    /// and reviewed main-purse payments. The product owns native lifecycle frames,
    /// subscriptions, delivery, retries, history, and acknowledgments.
    ///
    /// `Bind` resolves the peer independently. `Prepare` validates native plaintext
    /// and returns signed ciphertext for product submission. `Open` authenticates
    /// complete external statements and rejects reflected local output; it is not
    /// an arbitrary decryption primitive. Incoming plaintext can contain incoming
    /// bearer coin keys: persist the import intent securely and use generic payment
    /// top-up before acknowledging. Wallet/device keys and outgoing main-purse
    /// coin secrets never leave the Host.
    ///
    /// `Initialize` also advances private file transfers. Persist any legacy
    /// migration view and ordinary prepared statements before `CommitMigration`.
    /// `ContinueOpen` retrieves the next bounded page of an authenticated batch.
    /// `ContinueState` retrieves remaining pages of a stable public state snapshot;
    /// persist every page before committing its migration.
    ///
    /// Method 11 (the former raw-crypto interface) and method 12's former V1 actor
    /// operations are retired, not forwarded. This boundary uses V2 payloads.
    ///
    /// ```ts
    /// const result = await truapi.account.deviceChat({ tag: "Initialize" });
    /// assert(result.isOk(), "deviceChat failed:", result);
    /// console.log("Host-owned Chat device:", result.value.device);
    /// ```
    #[wire(id = 12)]
    async fn product_device_chat(
        &self,
        _cx: &CallContext,
        _request: HostProductDeviceChatRequest,
    ) -> Result<HostProductDeviceChatResponse, CallError<HostProductDeviceChatError>> {
        Err(CallError::unavailable())
    }

    /// List non-product accounts the user owns.
    ///
    /// Current hosts do not expose non-product accounts, so the list is empty.
    ///
    /// ```ts
    /// const result = await truapi.account.getLegacyAccounts();
    /// assert(result.isOk(), "getLegacyAccounts failed:", result);
    /// assert(result.value.accounts.length === 0, "unexpected legacy accounts:", result.value);
    /// console.log("legacy accounts:", result.value.accounts);
    /// ```
    #[wire(id = 4)]
    async fn get_legacy_accounts(
        &self,
        _cx: &CallContext,
        _request: HostGetLegacyAccountsRequest,
    ) -> Result<HostGetLegacyAccountsResponse, CallError<HostGetLegacyAccountsError>> {
        Err(CallError::unavailable())
    }

    /// Fetch the user's primary identity.
    ///
    /// ```ts
    /// const result = await truapi.account.getUserId();
    /// assert(result.isOk(), "getUserId failed:", result);
    /// console.log("user id:", result.value);
    /// ```
    #[wire(id = 5)]
    async fn get_user_id(
        &self,
        _cx: &CallContext,
        _request: HostGetUserIdRequest,
    ) -> Result<HostGetUserIdResponse, CallError<HostGetUserIdError>> {
        Err(CallError::unavailable())
    }

    /// Request the host to present the login flow to the user.
    ///
    /// Products should call this in response to a user action (e.g. tapping a
    /// "Sign in" button), not automatically on load.
    ///
    /// ```ts
    /// const result = await truapi.account.requestLogin({
    ///   reason: "Sign in to vote on Referendum #42",
    /// });
    /// assert(result.isOk(), "requestLogin failed:", result);
    /// console.log("login completed:", result.value);
    /// ```
    #[wire(id = 6)]
    async fn request_login(
        &self,
        _cx: &CallContext,
        _request: HostRequestLoginRequest,
    ) -> Result<HostRequestLoginResponse, CallError<HostRequestLoginError>> {
        Err(CallError::unavailable())
    }
}
