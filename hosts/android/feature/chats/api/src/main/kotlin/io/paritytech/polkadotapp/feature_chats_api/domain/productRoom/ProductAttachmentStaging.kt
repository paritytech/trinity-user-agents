package io.paritytech.polkadotapp.feature_chats_api.domain.productRoom

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel

/** Uploads a user attachment of a room of the chat product to HOP through the TrUAPI core. */
interface ProductAttachmentStaging {
    /** Runs [block] while the worker of the product of [chatId] runs, so the product answers the message that follows. */
    suspend fun <T> holdingWorker(chatId: ChatId, block: suspend () -> T): T

    /**
     * Uploads [bytes] to a HOP node as the native chat sends a file, in a room of the chat product that declares
     * `hop_attachments`. Gives the resource that names the file, or a [ProductAttachmentStageException]. [progress] gets
     * the bytes that the node holds and the size of the file, after each part, as the native upload counts its chunks.
     */
    suspend fun uploadToHop(
        chatId: ChatId,
        bytes: ByteArray,
        progress: (sent: Long, total: Long) -> Unit = { _, _ -> },
    ): Result<ChatResourceModel.Hop>
}

/** Why the core did not upload an attachment (`NativeChatStageRejection`). */
sealed class ProductAttachmentStageException(message: String) : Exception(message) {
    /** The room does not declare `hop_attachments`. */
    class NotDeclared : ProductAttachmentStageException("the room does not declare HOP attachments")

    /** The attachment is larger than the core uploads. */
    class TooLarge(val maxBytes: Long) : ProductAttachmentStageException("the attachment is larger than $maxBytes bytes")

    /** The upload failed, or the core is not available. */
    class SubmitFailed(val reason: String) : ProductAttachmentStageException("the upload failed: $reason")
}
