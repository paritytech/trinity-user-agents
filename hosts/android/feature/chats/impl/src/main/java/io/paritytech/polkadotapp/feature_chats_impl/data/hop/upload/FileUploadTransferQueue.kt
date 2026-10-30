package io.paritytech.polkadotapp.feature_chats_impl.data.hop.upload

import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.HopNodeUrlProvider
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.ConcurrentTransferQueue
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.RetryDecision
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.FileUploadRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.hop.FileUpload
import io.paritytech.polkadotapp.feature_chats_impl.domain.hop.toFileUploadError
import kotlinx.coroutines.flow.Flow
import timber.log.Timber
import javax.inject.Inject
import kotlin.time.Instant

class FileUploadTransferQueue @Inject constructor(
    private val repository: FileUploadRepository,
    private val uploader: HopFileUploader,
    private val hopNodeUrlProvider: HopNodeUrlProvider,
    uploadConcurrency: UploadConcurrency
) : ConcurrentTransferQueue<FileUpload> {
    override val maxInFlight: Int = uploadConcurrency.maxInFlight()

    override suspend fun claimNextDue(): FileUpload? = repository.claimNextDue()
    override suspend fun releaseInProgress() = repository.releaseInProgress()
    override fun queueChanges(): Flow<Unit> = repository.observeQueueChanges()
    override suspend fun nextBackoffEndAt(): Instant? = repository.getNextBackoffEndAt()
    override suspend fun process(item: FileUpload) = uploader.upload(item)
    override suspend fun markDone(item: FileUpload) = repository.markDone(item.messageId)
    override suspend fun markFailed(item: FileUpload, error: Throwable) =
        repository.markFailed(item.messageId, error.toFileUploadError())

    override suspend fun scheduleRetry(item: FileUpload, reschedule: RetryDecision.Reschedule) {
        repository.scheduleRetry(
            messageId = item.messageId,
            nodeUrl = retryNodeUrl(item, reschedule.attemptCount),
            attemptCount = reschedule.attemptCount,
            firstFailureAt = reschedule.firstFailureAt,
            nextAttemptAt = reschedule.nextAttemptAt
        )
    }

    private suspend fun retryNodeUrl(item: FileUpload, failures: Int): String {
        if (failures % FAILURES_BEFORE_NODE_SWITCH != 0) return item.nodeUrl

        return runCancellableCatching { hopNodeUrlProvider.pickForRetry(failedNodeUrl = item.nodeUrl) }
            .onFailure { Timber.w(it, "No Hop node to move upload ${item.messageId} to; retrying on ${item.nodeUrl}") }
            .getOrDefault(item.nodeUrl)
    }

    private companion object {
        const val FAILURES_BEFORE_NODE_SWITCH = 2
    }
}
