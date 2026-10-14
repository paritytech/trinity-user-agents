package io.paritytech.polkadotapp.feature_chats_impl.data.repository

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.database.dao.FileDownloadDao
import io.paritytech.polkadotapp.database.model.FileDownloadLocal
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.HopTicket
import io.paritytech.polkadotapp.feature_chats_impl.domain.hop.FileDownload
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.toLocal
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject
import kotlin.time.Instant

class FileDownloadRepository @Inject constructor(
    private val dao: FileDownloadDao
) {
    fun observeActive(): Flow<List<FileDownload>> {
        return dao.observeActive().map { downloads -> downloads.map { it.toDomain() } }
    }

    suspend fun addDownloadToQueue(download: FileDownload) {
        dao.insert(download.toLocal())
    }

    suspend fun getNextPending(): FileDownload? {
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

    suspend fun redownload(messageId: ChatMessageId) {
        dao.resetToPending(messageId)
    }

    suspend fun isActive(messageId: ChatMessageId): Boolean {
        return dao.isInProgress(messageId)
    }

    suspend fun cancel(messageId: ChatMessageId) {
        dao.markCancelled(messageId)
    }

    suspend fun markInProgress(messageId: ChatMessageId) {
        dao.markInProgress(messageId)
    }

    suspend fun resolveInline(
        messageId: ChatMessageId,
        filePath: String
    ) {
        dao.resolveInline(
            messageId = messageId,
            filePath = filePath
        )
    }

    suspend fun resolveChunked(
        messageId: ChatMessageId,
        chunkHashes: List<String>
    ) {
        dao.resolveChunked(
            messageId = messageId,
            chunkHashes = chunkHashes.joinToString(",")
        )
    }

    suspend fun updateProgress(
        messageId: ChatMessageId,
        downloadedChunks: Int,
        filePath: String
    ) {
        dao.updateProgress(
            messageId = messageId,
            downloadedChunks = downloadedChunks,
            filePath = filePath
        )
    }

    suspend fun markDone(messageId: ChatMessageId) {
        dao.markDone(messageId)
    }

    suspend fun markFailed(messageId: ChatMessageId, error: FileDownload.Error) {
        dao.markFailed(messageId, error.category.toLocal(), error.cause)
    }
}

private fun FileDownload.toLocal(): FileDownloadLocal {
    val payload = progress.payload
    val (downloadedChunks, chunkHashes) = when (payload) {
        is FileDownload.Payload.Chunked -> payload.downloadedChunks to payload.chunkHashes.joinToString(",")
        FileDownload.Payload.Inline, FileDownload.Payload.Unresolved -> 0 to null
    }

    return FileDownloadLocal(
        messageId = messageId,
        chatId = chatId.toLocal(),
        identifier = identifier.value,
        ticket = ticket.bytes,
        nodeUrl = nodeUrl,
        mimeType = mimeType,
        filePath = filePath,
        payloadKind = payload.toLocalKind(),
        downloadedChunks = downloadedChunks,
        chunkHashes = chunkHashes,
        status = progress.status.toLocal(),
        errorCategory = progress.error?.category?.toLocal(),
        errorCause = progress.error?.cause,
        createdAt = createdAt,
        retryState = progress.retryState.toLocal()
    )
}

private fun FileDownloadLocal.toDomain() = FileDownload(
    messageId = messageId,
    chatId = ChatId.fromRawValue(chatId),
    identifier = identifier.toDataByteArray(),
    ticket = HopTicket.fromRaw(ticket),
    nodeUrl = nodeUrl,
    mimeType = mimeType,
    filePath = filePath,
    progress = FileDownload.Progress(
        status = status.toDomain(),
        payload = toPayload(),
        error = errorCategory?.let { FileDownload.Error(category = it.toDomain(), cause = errorCause.orEmpty()) },
        retryState = retryState.toDomain()
    ),
    createdAt = createdAt
)

private fun FileDownload.Payload.toLocalKind() = when (this) {
    FileDownload.Payload.Unresolved -> FileDownloadLocal.PayloadKind.UNRESOLVED
    FileDownload.Payload.Inline -> FileDownloadLocal.PayloadKind.INLINE
    is FileDownload.Payload.Chunked -> FileDownloadLocal.PayloadKind.CHUNKED
}

private fun FileDownloadLocal.toPayload(): FileDownload.Payload = when (payloadKind) {
    FileDownloadLocal.PayloadKind.UNRESOLVED -> FileDownload.Payload.Unresolved
    FileDownloadLocal.PayloadKind.INLINE -> FileDownload.Payload.Inline
    FileDownloadLocal.PayloadKind.CHUNKED -> FileDownload.Payload.Chunked(
        chunkHashes = chunkHashes.orEmpty().split(",").filter { it.isNotEmpty() },
        downloadedChunks = downloadedChunks
    )
}

private fun FileDownload.Status.toLocal() = when (this) {
    FileDownload.Status.PENDING -> FileDownloadLocal.Status.PENDING
    FileDownload.Status.IN_PROGRESS -> FileDownloadLocal.Status.IN_PROGRESS
    FileDownload.Status.DONE -> FileDownloadLocal.Status.DONE
    FileDownload.Status.FAILED -> FileDownloadLocal.Status.FAILED
    FileDownload.Status.CANCELLED -> FileDownloadLocal.Status.CANCELLED
}

private fun FileDownloadLocal.Status.toDomain() = when (this) {
    FileDownloadLocal.Status.PENDING -> FileDownload.Status.PENDING
    FileDownloadLocal.Status.IN_PROGRESS -> FileDownload.Status.IN_PROGRESS
    FileDownloadLocal.Status.DONE -> FileDownload.Status.DONE
    FileDownloadLocal.Status.FAILED -> FileDownload.Status.FAILED
    FileDownloadLocal.Status.CANCELLED -> FileDownload.Status.CANCELLED
}

private fun FileDownload.Error.Category.toLocal() = when (this) {
    FileDownload.Error.Category.NETWORK -> FileDownloadLocal.ErrorCategory.NETWORK
    FileDownload.Error.Category.HOP_ERROR -> FileDownloadLocal.ErrorCategory.HOP_ERROR
    FileDownload.Error.Category.FILE_WRITE_ERROR -> FileDownloadLocal.ErrorCategory.FILE_WRITE_ERROR
    FileDownload.Error.Category.UNKNOWN -> FileDownloadLocal.ErrorCategory.UNKNOWN
}

private fun FileDownloadLocal.ErrorCategory.toDomain() = when (this) {
    FileDownloadLocal.ErrorCategory.NETWORK -> FileDownload.Error.Category.NETWORK
    FileDownloadLocal.ErrorCategory.HOP_ERROR -> FileDownload.Error.Category.HOP_ERROR
    FileDownloadLocal.ErrorCategory.FILE_WRITE_ERROR -> FileDownload.Error.Category.FILE_WRITE_ERROR
    FileDownloadLocal.ErrorCategory.UNKNOWN -> FileDownload.Error.Category.UNKNOWN
}
