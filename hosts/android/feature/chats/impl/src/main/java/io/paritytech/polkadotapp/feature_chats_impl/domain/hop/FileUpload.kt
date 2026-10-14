package io.paritytech.polkadotapp.feature_chats_impl.domain.hop

import android.net.Uri
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.HopTicket
import java.io.FileNotFoundException
import java.io.IOException

class FileUpload(
    val messageId: ChatMessageId,
    val chatId: ChatId,
    val ticket: HopTicket,
    val nodeUrl: String,
    val size: Size,
    val meta: Meta,
    val progress: Progress,
    val createdAt: Timestamp
) : WithRetryState {
    override val retryState: HopTransferRetryState
        get() = progress.retryState

    sealed interface Size {
        data object NotMeasured : Size
        data class Measured(val size: InformationSize) : Size
    }

    data class Meta(
        val uri: Uri,
        val mimeType: String
    )

    data class Progress(
        val status: Status,
        val totalChunks: Int,
        val uploadedChunks: Int,
        val uploadedChunkHashes: List<String>,
        val error: Error?,
        val retryState: HopTransferRetryState
    )

    enum class Status {
        PENDING, IN_PROGRESS, DONE, FAILED
    }

    data class Error(
        val category: Category,
        val cause: String
    ) {
        enum class Category {
            NETWORK,
            FILE_NOT_FOUND,
            HOP_ERROR,
            UNKNOWN
        }
    }

    companion object {
        fun new(
            messageId: ChatMessageId,
            chatId: ChatId,
            fileUri: Uri,
            mimeType: String,
            nodeUrl: String
        ): FileUpload {
            return FileUpload(
                messageId = messageId,
                chatId = chatId,
                ticket = HopTicket.random(),
                nodeUrl = nodeUrl,
                size = Size.NotMeasured,
                meta = Meta(
                    uri = fileUri,
                    mimeType = mimeType
                ),
                progress = Progress(
                    status = Status.PENDING,
                    totalChunks = 0,
                    uploadedChunks = 0,
                    uploadedChunkHashes = emptyList(),
                    error = null,
                    retryState = HopTransferRetryState.None
                ),
                createdAt = System.currentTimeMillis()
            )
        }
    }
}

fun Throwable.toFileUploadError(): FileUpload.Error {
    val category = when (this) {
        is FileNotFoundException, is SecurityException -> FileUpload.Error.Category.FILE_NOT_FOUND
        is IOException -> FileUpload.Error.Category.NETWORK
        else -> FileUpload.Error.Category.UNKNOWN
    }

    return FileUpload.Error(category, toString())
}
