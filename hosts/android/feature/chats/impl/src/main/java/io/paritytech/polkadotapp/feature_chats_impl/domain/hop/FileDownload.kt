package io.paritytech.polkadotapp.feature_chats_impl.domain.hop

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.HopTicket
import java.io.IOException

class FileDownload(
    val messageId: ChatMessageId,
    val chatId: ChatId,
    val identifier: DataByteArray,
    val ticket: HopTicket,
    val nodeUrl: String,
    val mimeType: String,
    val filePath: String?,
    val progress: Progress,
    val createdAt: Timestamp
) : WithRetryState {
    override val retryState: HopTransferRetryState
        get() = progress.retryState

    data class Progress(
        val status: Status,
        val payload: Payload,
        val error: Error?,
        val retryState: HopTransferRetryState
    )

    sealed interface Payload {
        data object Unresolved : Payload
        data object Inline : Payload
        data class Chunked(val chunkHashes: List<String>, val downloadedChunks: Int) : Payload
    }

    enum class Status {
        PENDING, IN_PROGRESS, DONE, FAILED, CANCELLED
    }

    data class Error(
        val category: Category,
        val cause: String
    ) {
        enum class Category {
            NETWORK,
            HOP_ERROR,
            FILE_WRITE_ERROR,
            UNKNOWN
        }
    }

    companion object {
        fun new(
            messageId: ChatMessageId,
            chatId: ChatId,
            identifier: DataByteArray,
            ticket: HopTicket,
            nodeUrl: String,
            mimeType: String
        ): FileDownload {
            return FileDownload(
                messageId = messageId,
                chatId = chatId,
                identifier = identifier,
                ticket = ticket,
                nodeUrl = nodeUrl,
                mimeType = mimeType,
                filePath = null,
                progress = Progress(
                    status = Status.PENDING,
                    payload = Payload.Unresolved,
                    error = null,
                    retryState = HopTransferRetryState.None
                ),
                createdAt = System.currentTimeMillis()
            )
        }
    }
}

fun Throwable.toFileDownloadError(): FileDownload.Error {
    val category = when (this) {
        is IOException -> FileDownload.Error.Category.NETWORK
        else -> FileDownload.Error.Category.UNKNOWN
    }

    return FileDownload.Error(category, toString())
}
