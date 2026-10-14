package io.paritytech.polkadotapp.feature_chats_impl.presentation.transfer

import io.paritytech.polkadotapp.common.utils.Fraction
import io.paritytech.polkadotapp.common.utils.Fraction.Companion.percents
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.presentation.transfer.FileTransferDirection
import io.paritytech.polkadotapp.feature_chats_api.presentation.transfer.FileTransferUiState
import io.paritytech.polkadotapp.feature_chats_impl.domain.hop.FileDownload
import io.paritytech.polkadotapp.feature_chats_impl.domain.hop.FileUpload
import javax.inject.Inject

class FileTransferUiStateMapper @Inject constructor() {
    fun merge(
        uploads: List<FileUpload>,
        downloads: List<FileDownload>
    ): Map<ChatMessageId, FileTransferUiState> = buildMap(uploads.size + downloads.size) {
        uploads.forEach { upload -> upload.toUiState()?.let { put(upload.messageId, it) } }
        downloads.forEach { download -> download.toUiState()?.let { put(download.messageId, it) } }
    }

    private fun FileUpload.toUiState(): FileTransferUiState? = when (progress.status) {
        FileUpload.Status.PENDING,
        FileUpload.Status.IN_PROGRESS -> FileTransferUiState.InProgress(
            direction = FileTransferDirection.UPLOAD,
            progress = progressFraction(progress.uploadedChunks, progress.totalChunks)
        )

        FileUpload.Status.FAILED -> FileTransferUiState.Failed(FileTransferDirection.UPLOAD)
        FileUpload.Status.DONE -> null
    }

    private fun FileDownload.toUiState(): FileTransferUiState? = when (progress.status) {
        FileDownload.Status.PENDING,
        FileDownload.Status.IN_PROGRESS -> FileTransferUiState.InProgress(
            direction = FileTransferDirection.DOWNLOAD,
            progress = progress.payload.toFraction()
        )

        FileDownload.Status.FAILED -> FileTransferUiState.Failed(FileTransferDirection.DOWNLOAD)
        FileDownload.Status.CANCELLED -> FileTransferUiState.Cancelled(FileTransferDirection.DOWNLOAD)
        FileDownload.Status.DONE -> null
    }

    private fun FileDownload.Payload.toFraction(): Fraction = when (this) {
        is FileDownload.Payload.Chunked -> progressFraction(downloadedChunks, chunkHashes.size)
        FileDownload.Payload.Inline,
        FileDownload.Payload.Unresolved -> 0.percents
    }

    private fun progressFraction(done: Int, total: Int): Fraction {
        if (total <= 0) return 0.percents
        return (done * 100 / total).coerceIn(0, 100).percents
    }
}
