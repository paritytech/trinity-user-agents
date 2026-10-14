package io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol

import io.paritytech.polkadotapp.bandersnatch_crypto.ContextualAlias
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId
import io.paritytech.polkadotapp.feature_products_api.model.ProductId

interface AccountsProtocol {
    /**
     * Returned list matches [resources] in length and order.
     * Single user-authorization round-trip per call.
     */
    suspend fun requestResourceAllocation(
        callingProduct: ProductId,
        resources: List<ApAllocatableResource>,
        onExisting: OnExistingAllowancePolicy,
    ): List<ApAllocationOutcome>

    /**
     * RFC-0024: registers a ring VRF key the calling product owns, declaring the ring it is intended
     * for, and returns its member public key. Registering an already-known [index] for a further
     * [ring] extends the existing entry rather than creating a second one.
     *
     * A product may only register its own keys, so ownership is [callingProductId] and never a
     * parameter. Registration declares intent, not membership.
     */
    suspend fun registerRingVrfKey(
        callingProductId: ProductId,
        index: DerivationIndex32,
        ring: RingLocation,
    ): Result<DataByteArray>

    /**
     * RFC-0024: lists the registry entries owned by [owner], which may be the caller or another
     * product. Handles are opaque — consumers select by declared ring, never by index.
     */
    suspend fun listRingVrfKeys(
        callingProductId: ProductId,
        owner: ProductId,
        disclosure: RingVrfKeyDisclosure,
    ): Result<List<RegisteredRingVrfKey>>

    /**
     * Derives the contextual alias for [context] using the key named by [keyHandle].
     * Derive-only: does not validate ring membership, so an alias is returned even before
     * the user reaches full personhood. Failures are surfaced as [GetAliasError], sharing
     * create_proof's ring-resolution failure modes.
     */
    suspend fun getContextualAlias(
        callingProductId: ProductId,
        keyHandle: ProductAccountId,
        context: ProductProofContext,
        ring: RingLocation,
    ): Result<ContextualAlias>

    /**
     * Creates a ring-VRF proof for [message] scoped to [context] within [ring], using the key named
     * by [keyHandle]. When [context]'s product differs from [callingProductId] the user is prompted
     * to approve the cross-product action. Failures are surfaced as [CreateProofError].
     */
    suspend fun createProof(
        callingProductId: ProductId,
        keyHandle: ProductAccountId,
        context: ProductProofContext,
        ring: RingLocation,
        message: ByteArray,
    ): Result<RingVrfProof>

    /**
     * RFC-0024: signs [message] with the member key named by [keyHandle], producing an ordinary
     * signature rather than an anonymous ring proof. It derives no alias and proves no membership,
     * so there is no ring or context to scope it — the signature is linkable to every other use of
     * that key, and verifying it needs the member public key.
     */
    suspend fun ringVrfSign(
        callingProductId: ProductId,
        keyHandle: ProductAccountId,
        message: ByteArray,
    ): Result<ByteArray>

    /**
     * Produces an sr25519 VRF signature from [account] over the merlin transcript described by
     * [transcriptLabel] and [items]: `Transcript::new(transcriptLabel)` followed by one
     * `append_message(item.label, item.value)` per item, in order (RFC-0023). The transcript is
     * replayed verbatim — labels and values are never interpreted.
     *
     * Every call takes a user confirmation; declining yields [SignVrfError.Rejected]. Oversized
     * transcripts are refused before the confirmation with [SignVrfError.TranscriptTooLarge].
     */
    suspend fun signVrf(
        callingProductId: ProductId,
        account: ProductAccountId,
        transcriptLabel: ByteArray,
        items: List<VrfTranscriptItem>,
    ): Result<VrfSignature>
}

suspend fun AccountsProtocol.requestResourceAllocation(
    callingProduct: ProductId,
    resource: ApAllocatableResource,
    onExisting: OnExistingAllowancePolicy,
): ApAllocationOutcome {
    return requestResourceAllocation(callingProduct, listOf(resource), onExisting).single()
}
