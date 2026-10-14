package io.paritytech.polkadotapp.feature_transactions_impl.data.tracked

import android.content.Context
import androidx.work.BackoffPolicy
import androidx.work.Constraints
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.OutOfQuotaPolicy
import androidx.work.WorkManager
import dagger.hilt.android.qualifiers.ApplicationContext
import java.util.concurrent.TimeUnit
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class TrackedExtrinsicEnqueuer @Inject constructor(
    @param:ApplicationContext private val context: Context,
) {
    /**
     * Enqueues the watcher as unique work with [ExistingWorkPolicy.KEEP]: a running watcher already picks up
     * newly-persisted rows on its next sweep, so a concurrent submit must not restart or duplicate it.
     */
    fun enqueue() {
        val request = OneTimeWorkRequestBuilder<TrackedExtrinsicWorker>()
            .setExpedited(OutOfQuotaPolicy.RUN_AS_NON_EXPEDITED_WORK_REQUEST)
            .setBackoffCriteria(BackoffPolicy.LINEAR, 30, TimeUnit.SECONDS)
            .setConstraints(
                Constraints.Builder()
                    .setRequiredNetworkType(NetworkType.CONNECTED)
                    .build()
            )
            .build()

        WorkManager.getInstance(context)
            .enqueueUniqueWork(TrackedExtrinsicWorker.WORK_ID, ExistingWorkPolicy.KEEP, request)
    }
}
