package io.paritytech.polkadotapp.feature_chats_impl.data.hop.upload

import android.content.Context
import androidx.hilt.work.HiltWorker
import androidx.work.CoroutineWorker
import androidx.work.ForegroundInfo
import androidx.work.WorkerParameters
import dagger.assisted.Assisted
import dagger.assisted.AssistedInject
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.common.utils.toWorkerResult
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.TransferQueueProcessor
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.enqueueDelayedTransfer
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.enqueueExpeditedTransfer
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.hopTransferForegroundInfo
import timber.log.Timber
import io.paritytech.polkadotapp.common.R as RCommon

@HiltWorker
class FileUploadWorker @AssistedInject constructor(
    @Assisted appContext: Context,
    @Assisted parameters: WorkerParameters,
    private val transferQueueProcessor: TransferQueueProcessor,
    private val transferQueue: FileUploadTransferQueue
) : CoroutineWorker(appContext, parameters) {
    override suspend fun doWork(): Result {
        runCancellableCatching { setForeground(getForegroundInfo()) }
            .onFailure { Timber.d(it, "Failed to promote upload worker to foreground; continuing as background work") }

        val processed = runCancellableCatching { transferQueueProcessor.processPending(transferQueue) }

        transferQueue.soonestNextAttemptAt()?.let { soonest ->
            enqueueDelayedTransfer<FileUploadWorker>(applicationContext, WORK_ID, soonest)
        }

        return processed.toWorkerResult(retryOnFailure = true)
    }

    override suspend fun getForegroundInfo(): ForegroundInfo = hopTransferForegroundInfo(
        context = applicationContext,
        notificationId = NOTIFICATION_ID,
        titleRes = RCommon.string.chat_file_upload_worker_notification_title,
        messageRes = RCommon.string.chat_file_upload_worker_notification_message
    )

    companion object {
        private const val WORK_ID = "HopFileUpload"
        private const val NOTIFICATION_ID = 1002

        fun startUploadingWork(context: Context) = enqueueExpeditedTransfer<FileUploadWorker>(context, WORK_ID)
    }
}
