package io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox

import android.content.Context
import androidx.hilt.work.HiltWorker
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingWorkPolicy
import androidx.work.ForegroundInfo
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.OutOfQuotaPolicy
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import androidx.work.workDataOf
import dagger.assisted.Assisted
import dagger.assisted.AssistedInject
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.hopTransferForegroundInfo
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.attachments.ProductAttachmentStageScheduler
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.attachments.ProductAttachmentStager
import timber.log.Timber
import javax.inject.Inject
import io.paritytech.polkadotapp.common.R as RCommon

/** Stages one entry of the attachment outbox. The job continues after the room screen closes. */
@HiltWorker
class ProductAttachmentStageWorker @AssistedInject constructor(
    @Assisted appContext: Context,
    @Assisted parameters: WorkerParameters,
    private val stager: ProductAttachmentStager,
) : CoroutineWorker(appContext, parameters) {
    override suspend fun doWork(): Result {
        val entryId = inputData.getString(KEY_ENTRY_ID) ?: return Result.failure()

        runCancellableCatching { setForeground(getForegroundInfo()) }
            .onFailure { Timber.d(it, "The attachment stage job runs as background work") }

        // The stager keeps each failure in the outbox entry, where the user can retry it.
        stager.process(entryId)
        return Result.success()
    }

    override suspend fun getForegroundInfo(): ForegroundInfo = hopTransferForegroundInfo(
        context = applicationContext,
        notificationId = NOTIFICATION_ID,
        titleRes = RCommon.string.chat_file_upload_worker_notification_title,
        messageRes = RCommon.string.chat_file_upload_worker_notification_message,
    )

    companion object {
        const val KEY_ENTRY_ID = "entry_id"
        private const val NOTIFICATION_ID = 1004

        fun workName(entryId: ChatMessageId) = "ProductAttachmentStage:$entryId"
    }
}

class WorkManagerProductAttachmentStageScheduler @Inject constructor(
    @param:ApplicationContext private val context: Context,
) : ProductAttachmentStageScheduler {
    override fun schedule(entryId: ChatMessageId) {
        val request = OneTimeWorkRequestBuilder<ProductAttachmentStageWorker>()
            .setExpedited(OutOfQuotaPolicy.RUN_AS_NON_EXPEDITED_WORK_REQUEST)
            .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
            .setInputData(workDataOf(ProductAttachmentStageWorker.KEY_ENTRY_ID to entryId))
            .build()

        // A retry that comes while the last job of the entry ends runs after that job. It does not stop it.
        WorkManager.getInstance(context)
            .enqueueUniqueWork(ProductAttachmentStageWorker.workName(entryId), ExistingWorkPolicy.APPEND_OR_REPLACE, request)
    }

    override fun cancel(entryId: ChatMessageId) {
        WorkManager.getInstance(context).cancelUniqueWork(ProductAttachmentStageWorker.workName(entryId))
    }
}
