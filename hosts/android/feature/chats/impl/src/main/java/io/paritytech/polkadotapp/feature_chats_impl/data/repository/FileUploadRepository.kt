package io.paritytech.polkadotapp.feature_chats_impl.data.repository

import androidx.core.net.toUri
import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.bytes
import io.paritytech.polkadotapp.database.dao.FileUploadDao
import io.paritytech.polkadotapp.database.model.FileUploadLocal
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.HopTicket
import io.paritytech.polkadotapp.feature_chats_impl.domain.hop.FileUpload
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.toLocal
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject
import kotlin.time.Instant

class FileUploadRepository @Inject constructor(
    private val dao: FileUploadDao
) {
    fun observeActive(): Flow<List<FileUpload>> {
        return dao.observeActive().map { uploads -> uploads.map { it.toDomain() } }
    }

    suspend fun addUploadToQueue(upload: FileUpload) {
        dao.insert(upload.toLocal())
    }

    suspend fun getNextPending(): FileUpload? {
        return dao.getNextPending(System.currentTimeMillis())?.toDomain()
    }

    suspend fun scheduleRetry(
        messageId: ChatMessageId,
        attemptCount: Int,
        firstFailureAt: Instant,
        nextAttemptAt: Instant
    ) {
        dao.scheduleRetry(messageId, attemptCount, firstFailureAt.toEpochMilliseconds(), nextAttemptAt.toEpochMilliseconds())
    }

    suspend fun getSoonestNextAttemptAt(): Instant? {
        return dao.getSoonestNextAttemptAt()?.let(Instant::fromEpochMilliseconds)
    }

    suspend fun isActive(messageId: ChatMessageId): Boolean {
        return dao.isInProgress(messageId)
    }

    suspend fun markInProgress(messageId: ChatMessageId) {
        dao.markInProgress(messageId)
    }

    suspend fun updateFileInfo(messageId: ChatMessageId, fileSize: InformationSize, totalChunks: Int) {
        dao.updateFileInfo(messageId, fileSize.inWholeBytes, totalChunks)
    }

    suspend fun updateProgress(messageId: ChatMessageId, uploadedChunks: Int, chunkHashes: List<String>) {
        dao.updateProgress(messageId, uploadedChunks, chunkHashes.joinToString(","))
    }

    suspend fun markDone(messageId: ChatMessageId) {
        dao.markDone(messageId)
    }

    suspend fun markFailed(messageId: ChatMessageId, error: FileUpload.Error) {
        dao.markFailed(messageId, error.category.toLocal(), error.cause)
    }
}

private fun FileUpload.toLocal() = FileUploadLocal(
    messageId = messageId,
    chatId = chatId.toLocal(),
    fileUri = meta.uri.toString(),
    mimeType = meta.mimeType,
    fileSize = (size as? FileUpload.Size.Measured)?.size?.inWholeBytes,
    totalChunks = progress.totalChunks,
    uploadedChunks = progress.uploadedChunks,
    ticket = ticket.bytes,
    nodeUrl = nodeUrl,
    chunkHashes = progress.uploadedChunkHashes.takeIf { it.isNotEmpty() }?.joinToString(","),
    status = progress.status.toLocal(),
    errorCategory = progress.error?.category?.toLocal(),
    errorCause = progress.error?.cause,
    createdAt = createdAt,
    retryState = progress.retryState.toLocal()
)

private fun FileUploadLocal.toDomain() = FileUpload(
    messageId = messageId,
    chatId = ChatId.fromRawValue(chatId),
    ticket = HopTicket.fromRaw(ticket),
    nodeUrl = nodeUrl,
    size = fileSize?.bytes?.let(FileUpload.Size::Measured) ?: FileUpload.Size.NotMeasured,
    meta = FileUpload.Meta(
        uri = fileUri.toUri(),
        mimeType = mimeType
    ),
    progress = FileUpload.Progress(
        status = status.toDomain(),
        totalChunks = totalChunks,
        uploadedChunks = uploadedChunks,
        uploadedChunkHashes = chunkHashes?.split(",")?.filter { it.isNotEmpty() } ?: emptyList(),
        error = errorCategory?.let { FileUpload.Error(category = it.toDomain(), cause = errorCause.orEmpty()) },
        retryState = retryState.toDomain()
    ),
    createdAt = createdAt
)

private fun FileUpload.Status.toLocal() = when (this) {
    FileUpload.Status.PENDING -> FileUploadLocal.Status.PENDING
    FileUpload.Status.IN_PROGRESS -> FileUploadLocal.Status.IN_PROGRESS
    FileUpload.Status.DONE -> FileUploadLocal.Status.DONE
    FileUpload.Status.FAILED -> FileUploadLocal.Status.FAILED
}

private fun FileUploadLocal.Status.toDomain() = when (this) {
    FileUploadLocal.Status.PENDING -> FileUpload.Status.PENDING
    FileUploadLocal.Status.IN_PROGRESS -> FileUpload.Status.IN_PROGRESS
    FileUploadLocal.Status.DONE -> FileUpload.Status.DONE
    FileUploadLocal.Status.FAILED -> FileUpload.Status.FAILED
}

private fun FileUpload.Error.Category.toLocal() = when (this) {
    FileUpload.Error.Category.NETWORK -> FileUploadLocal.ErrorCategory.NETWORK
    FileUpload.Error.Category.FILE_NOT_FOUND -> FileUploadLocal.ErrorCategory.FILE_NOT_FOUND
    FileUpload.Error.Category.HOP_ERROR -> FileUploadLocal.ErrorCategory.HOP_ERROR
    FileUpload.Error.Category.UNKNOWN -> FileUploadLocal.ErrorCategory.UNKNOWN
}

private fun FileUploadLocal.ErrorCategory.toDomain() = when (this) {
    FileUploadLocal.ErrorCategory.NETWORK -> FileUpload.Error.Category.NETWORK
    FileUploadLocal.ErrorCategory.FILE_NOT_FOUND -> FileUpload.Error.Category.FILE_NOT_FOUND
    FileUploadLocal.ErrorCategory.HOP_ERROR -> FileUpload.Error.Category.HOP_ERROR
    FileUploadLocal.ErrorCategory.UNKNOWN -> FileUpload.Error.Category.UNKNOWN
}
