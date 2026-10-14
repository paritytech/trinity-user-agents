package io.paritytech.polkadotapp.feature_chats_impl.data.hop.upload

import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.RetryDecision
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.TransferQueue
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.FileUploadRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.hop.FileUpload
import io.paritytech.polkadotapp.feature_chats_impl.domain.hop.toFileUploadError
import javax.inject.Inject
import kotlin.time.Instant

class FileUploadTransferQueue @Inject constructor(
    private val repository: FileUploadRepository,
    private val uploader: HopFileUploader
) : TransferQueue<FileUpload> {
    override suspend fun nextPending(): FileUpload? = repository.getNextPending()
    override suspend fun markInProgress(item: FileUpload) = repository.markInProgress(item.messageId)
    override suspend fun process(item: FileUpload) = uploader.upload(item)
    override suspend fun markDone(item: FileUpload) = repository.markDone(item.messageId)
    override suspend fun markFailed(item: FileUpload, error: Throwable) =
        repository.markFailed(item.messageId, error.toFileUploadError())

    override suspend fun scheduleRetry(item: FileUpload, reschedule: RetryDecision.Reschedule) =
        repository.scheduleRetry(item.messageId, reschedule.attemptCount, reschedule.firstFailureAt, reschedule.nextAttemptAt)

    override suspend fun soonestNextAttemptAt(): Instant? = repository.getSoonestNextAttemptAt()
}
