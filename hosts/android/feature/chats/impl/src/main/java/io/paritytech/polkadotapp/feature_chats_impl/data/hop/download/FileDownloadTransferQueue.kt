package io.paritytech.polkadotapp.feature_chats_impl.data.hop.download

import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.RetryDecision
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.TransferQueue
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.FileDownloadRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.hop.FileDownload
import io.paritytech.polkadotapp.feature_chats_impl.domain.hop.toFileDownloadError
import javax.inject.Inject
import kotlin.time.Instant

class FileDownloadTransferQueue @Inject constructor(
    private val repository: FileDownloadRepository,
    private val downloader: HopFileDownloader
) : TransferQueue<FileDownload> {
    override suspend fun nextPending(): FileDownload? = repository.getNextPending()
    override suspend fun markInProgress(item: FileDownload) = repository.markInProgress(item.messageId)
    override suspend fun process(item: FileDownload) = downloader.download(item)
    override suspend fun markDone(item: FileDownload) = repository.markDone(item.messageId)
    override suspend fun markFailed(item: FileDownload, error: Throwable) =
        repository.markFailed(item.messageId, error.toFileDownloadError())

    override suspend fun scheduleRetry(item: FileDownload, reschedule: RetryDecision.Reschedule) =
        repository.scheduleRetry(item.messageId, reschedule.attemptCount, reschedule.firstFailureAt, reschedule.nextAttemptAt)

    override suspend fun soonestNextAttemptAt(): Instant? = repository.getSoonestNextAttemptAt()
}
