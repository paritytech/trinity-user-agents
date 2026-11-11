package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningAccount
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
        val account: SigningAccount,
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

    /** Let a product bind and use wallet-held Chat identity authority. */
    class ChatAuthority(
        override val requesterProductId: ProductId,
    ) : TrUAPIConfirmation.Prompt

    /** Share a profile reference with the user's Chat contacts. */
    class ProfileDisclosure(
        override val requesterProductId: ProductId,
    ) : TrUAPIConfirmation.Prompt

    /** Allocate host resources to the product. */
    class ResourceAllocation(
        override val requesterProductId: ProductId,
        val resources: List<String>,
    ) : TrUAPIConfirmation.Prompt

    /** Approve one exact payment from the user's main purse. */
    class MainPurseChatPayment(
        override val requesterProductId: ProductId,
        val recipient: String,
        val amountCents: ULong,
        val maxDebitCents: ULong,
        val genesisHash: String,
        val coinageInstanceId: UInt?,
        val operationId: String,
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

    /** Move NFT items out of a purse the requesting product does not own. */
    data class NftPurseDrain(
        override val requesterProductId: ProductId,
        val fromPurse: UInt,
        /** The product that owns the purse, or null when the user's own account does. */
        val ownerProductId: ProductId?,
        val itemCount: UInt,
        val destination: Destination,
    ) : TrUAPIConfirmation.Prompt {
        /** Where the items go. */
        sealed interface Destination {
            /** Fresh keys of another purse. */
            data class Purse(val purse: UInt) : Destination

            /** A key outside every purse, as hex. */
            data class ExternalKey(val keyHex: String) : Destination

            /** A cheque the product hands to another user. */
            data object Cheque : Destination

            /** The keys of a deposited cheque, back to its sender. */
            data object Refund : Destination
        }
    }
}
