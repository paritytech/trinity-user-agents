package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningRequestBody

/**
 * A core-initiated action awaiting the user's yes/no.
 *
 * Display-ready values rather than the core's FFI types: the confirmation UI
 * only ever renders and answers, and keeping `uniffi.*` out of presentation
 * means a pin bump cannot reach the screens.
 */
sealed interface TrUAPIConfirmation {
    /** Product the core is acting on behalf of, for the prompt's title. */
    val requesterProductId: String

    /**
     * Signing, in all four of its flavours. Goes to the app's own signing sheet
     * rather than a prompt, so the user sees the decoded call. Confirm-only:
     * the core holds the key and signs after approval.
     */
    class Signing(
        override val requesterProductId: String,
        val request: SigningRequestBody,
    ) : TrUAPIConfirmation

    /** Everything the app has no dedicated screen for, rendered generically. */
    sealed interface Prompt : TrUAPIConfirmation

    /** Sign a Statement Store proof payload with a product account. */
    class StatementSign(
        override val requesterProductId: String,
        val payloadSize: Int,
    ) : TrUAPIConfirmation.Prompt

    /** Derive a contextual alias for a ring. */
    class AccountAlias(
        override val requesterProductId: String,
        val proofContext: String,
        val ring: String,
    ) : TrUAPIConfirmation.Prompt

    /** Create a ring-VRF proof for a ring. */
    class CreateProof(
        override val requesterProductId: String,
        val proofContext: String,
        val ring: String,
        val messageSize: Int,
    ) : TrUAPIConfirmation.Prompt

    /** Disclose the user's primary identity to a product. */
    class IdentityDisclosure(
        override val requesterProductId: String,
    ) : TrUAPIConfirmation.Prompt

    /** Allocate host resources to the product. */
    class ResourceAllocation(
        override val requesterProductId: String,
        val resources: List<String>,
    ) : TrUAPIConfirmation.Prompt

    /** Submit a preimage to the host-selected backend. */
    class PreimageSubmit(
        override val requesterProductId: String,
        val sizeBytes: Long,
    ) : TrUAPIConfirmation.Prompt

    /** Let one product act on another product's account. */
    class AccountAccess(
        override val requesterProductId: String,
        val targetProductId: String,
    ) : TrUAPIConfirmation.Prompt

    /** Resolve a product's own account subtree over SSO. */
    class ProductSubtree(
        override val requesterProductId: String,
    ) : TrUAPIConfirmation.Prompt
}
