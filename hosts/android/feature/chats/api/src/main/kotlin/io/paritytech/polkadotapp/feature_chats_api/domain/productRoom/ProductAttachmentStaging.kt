package io.paritytech.polkadotapp.feature_chats_api.domain.productRoom

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId

/**
 * Stores a user attachment of a product room as one Bulletin preimage through the TrUAPI core (07-LATER §7). The
 * product pays for it from its own Bulletin allowance. The core keeps the allowance key and does the network work.
 */
interface ProductAttachmentStaging {
    /**
     * Runs [block] while the worker of the product of [chatId] runs. Through the worker, the core reads the features
     * of the room and asks the user for `PreimageSubmit`, and the product answers the message that follows.
     */
    suspend fun <T> holdingWorker(chatId: ChatId, block: suspend () -> T): T

    /** Gives the 32-byte key of the preimage, or a [ProductAttachmentStageException]. */
    suspend fun stage(chatId: ChatId, mimeType: String, bytes: ByteArray): Result<DataByteArray>
}

/** Why the core did not stage an attachment (`NativeChatStageRejection`). */
sealed class ProductAttachmentStageException(message: String) : Exception(message) {
    /** The room does not declare `preimage_attachments`. */
    class NotDeclared : ProductAttachmentStageException("the room does not declare preimage attachments")

    /** The attachment is larger than the core stages, or than one Bulletin transaction stores. */
    class TooLarge(val maxBytes: Long) : ProductAttachmentStageException("the attachment is larger than $maxBytes bytes")

    /** The user refused `PreimageSubmit` for the product. */
    class PermissionDenied : ProductAttachmentStageException("the user refused preimage submission for the product")

    /** The product has no Bulletin allowance. */
    class AllowanceUnavailable(val reason: String) : ProductAttachmentStageException("no Bulletin allowance: $reason")

    /** The submission failed, or the core is not available. */
    class SubmitFailed(val reason: String) : ProductAttachmentStageException("the submission failed: $reason")
}
