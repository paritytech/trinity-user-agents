package io.paritytech.polkadotapp.feature_chats_impl.data.hop.compaction

import android.content.Context
import androidx.hilt.work.HiltWorker
import androidx.work.CoroutineWorker
import androidx.work.WorkerParameters
import dagger.assisted.Assisted
import dagger.assisted.AssistedInject
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.common.utils.toWorkerResult
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.TransferQueueProcessor
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.enqueueDelayedTransfer
import io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer.enqueueQuietTransfer

@HiltWorker
class CompactionExpansionWorker @AssistedInject constructor(
    @Assisted appContext: Context,
    @Assisted parameters: WorkerParameters,
    private val transferQueueProcessor: TransferQueueProcessor,
    private val transferQueue: CompactionExpansionTransferQueue
) : CoroutineWorker(appContext, parameters) {
    override suspend fun doWork(): Result {
        val processed = runCancellableCatching { transferQueueProcessor.processPending(transferQueue) }

        transferQueue.soonestNextAttemptAt()?.let { soonest ->
            enqueueDelayedTransfer<CompactionExpansionWorker>(applicationContext, WORK_ID, soonest)
        }

        return processed.toWorkerResult(retryOnFailure = true)
    }

    companion object {
        private const val WORK_ID = "HopCompactionExpansion"

        fun startExpansionWork(context: Context) = enqueueQuietTransfer<CompactionExpansionWorker>(context, WORK_ID)
    }
}
