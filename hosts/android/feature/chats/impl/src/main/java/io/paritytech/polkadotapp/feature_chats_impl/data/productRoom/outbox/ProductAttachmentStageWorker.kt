package io.paritytech.polkadotapp.feature_chats_impl.data.productRoom.outbox

import android.content.Context
import androidx.hilt.work.HiltWorker
import androidx.work.BackoffPolicy
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingWorkPolicy
import androidx.work.ForegroundInfo
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.OutOfQuotaPolicy
import androidx.work.WorkInfo
import androidx.work.WorkManager
import androidx.work.WorkRequest
import androidx.work.WorkerParameters
import androidx.work.workDataOf
import dagger.assisted.Assisted
import dagger.assisted.AssistedInject
import dagger.hilt.android.qualifiers.ApplicationContext
import io.novasama.substrate_sdk_android.extensions.fromHex
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.hopTransferForegroundInfo
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.attachments.ProductAttachmentStageScheduler
import io.paritytech.polkadotapp.feature_chats_impl.domain.productRoom.attachments.ProductAttachmentStager
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import timber.log.Timber
import java.util.concurrent.TimeUnit
import javax.inject.Inject
import io.paritytech.polkadotapp.common.R as RCommon

/**
 * Sends the outbox of one room, in order. The job continues after the room screen closes and after the process stops,
 * and runs again with a backoff while a message did not go, as the native chat sends a message until it goes.
 */
@HiltWorker
class ProductAttachmentStageWorker @AssistedInject constructor(
    @Assisted appContext: Context,
    @Assisted parameters: WorkerParameters,
    private val stager: ProductAttachmentStager,
) : CoroutineWorker(appContext, parameters) {
    override suspend fun doWork(): Result {
        // A job of an earlier build named an entry, not a room. Its entry goes with the next send of its room.
        val chatId = inputData.getString(KEY_CHAT_ID)?.let { ChatId.fromRawValue(it.fromHex()) } ?: return Result.success()

        // Only an upload shows the notification of a file transfer.
        if (stager.hasUploads(chatId)) {
            runCancellableCatching { setForeground(getForegroundInfo()) }
                .onFailure { Timber.d(it, "The outbox job runs as background work") }
        }

        return if (stager.processRoom(chatId)) Result.success() else Result.retry()
    }

    override suspend fun getForegroundInfo(): ForegroundInfo = hopTransferForegroundInfo(
        context = applicationContext,
        notificationId = NOTIFICATION_ID,
        titleRes = RCommon.string.chat_file_upload_worker_notification_title,
        messageRes = RCommon.string.chat_file_upload_worker_notification_message,
    )

    companion object {
        const val KEY_CHAT_ID = "chat_id"
        private const val NOTIFICATION_ID = 1004

        fun workName(chatId: ChatId) = "ProductOutbox:${chatId.value.value.toHexString()}"
    }
}

class WorkManagerProductAttachmentStageScheduler @Inject constructor(
    @param:ApplicationContext private val context: Context,
) : ProductAttachmentStageScheduler {
    override suspend fun schedule(chatId: ChatId) {
        val request = OneTimeWorkRequestBuilder<ProductAttachmentStageWorker>()
            .setExpedited(OutOfQuotaPolicy.RUN_AS_NON_EXPEDITED_WORK_REQUEST)
            .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
            .setBackoffCriteria(BackoffPolicy.EXPONENTIAL, WorkRequest.MIN_BACKOFF_MILLIS, TimeUnit.MILLISECONDS)
            .setInputData(workDataOf(ProductAttachmentStageWorker.KEY_CHAT_ID to chatId.value.value.toHexString()))
            .build()

        // A send that comes while a job of the room runs goes after that job, so the messages of a room keep their order. A
        // job that waits for its backoff gives way, so a new message goes at once.
        val workManager = WorkManager.getInstance(context)
        val name = ProductAttachmentStageWorker.workName(chatId)
        val running = withContext(Dispatchers.IO) { workManager.getWorkInfosForUniqueWork(name).get() }
            .any { it.state == WorkInfo.State.RUNNING }
        val policy = if (running) ExistingWorkPolicy.APPEND_OR_REPLACE else ExistingWorkPolicy.REPLACE
        workManager.enqueueUniqueWork(name, policy, request)
    }
}
