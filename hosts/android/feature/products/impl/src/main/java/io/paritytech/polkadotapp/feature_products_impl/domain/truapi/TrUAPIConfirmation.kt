package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningRequestBody
import uniffi.truapi.ProductId

/**
 * A core-initiated action awaiting the user's yes/no.
 *
 * The UI holds value types and answers; the core retains signing authority.
 */
sealed interface TrUAPIConfirmation {
    /** Product the core is acting on behalf of, or null for a host action. */
    val requesterProductId: ProductId?

    /**
     * Signing, in all four of its flavours. Goes to the app's own signing sheet
     * rather than a prompt, so the user sees the decoded call. Confirm-only:
     * the core holds the key and signs after approval.
     */
    class Signing(
        override val requesterProductId: ProductId?,
        val request: SigningRequestBody,
    ) : TrUAPIConfirmation

    /** Everything the app has no dedicated screen for, rendered generically. */
    sealed interface Prompt : TrUAPIConfirmation {
        override val requesterProductId: ProductId
    }

    /** Sign a Statement Store proof payload with a product account. */
    class StatementSign(
        override val requesterProductId: ProductId,
        val payloadSize: Int,
    ) : TrUAPIConfirmation.Prompt

    /** Derive a contextual alias for a ring. */
    class AccountAlias(
        override val requesterProductId: ProductId,
        val proofContext: ProductId,
        val ring: String,
    ) : TrUAPIConfirmation.Prompt

    /** Create a ring-VRF proof for a ring. */
    class CreateProof(
        override val requesterProductId: ProductId,
        val proofContext: ProductId,
        val ring: String,
        val messageSize: Int,
    ) : TrUAPIConfirmation.Prompt

    /** Disclose the user's primary identity to a product. */
    class IdentityDisclosure(
        override val requesterProductId: ProductId,
    ) : TrUAPIConfirmation.Prompt

    /** Allocate host resources to the product. */
    class ResourceAllocation(
        override val requesterProductId: ProductId,
        val resources: List<String>,
    ) : TrUAPIConfirmation.Prompt

    /** Let one product act on another product's account. */
    class AccountAccess(
        override val requesterProductId: ProductId,
        val targetProductId: ProductId,
    ) : TrUAPIConfirmation.Prompt

    /** Resolve a product's own account subtree over SSO. */
    class ProductSubtree(
        override val requesterProductId: ProductId,
    ) : TrUAPIConfirmation.Prompt
}
