package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import android.content.pm.ServiceInfo
import androidx.core.app.NotificationCompat
import androidx.hilt.work.HiltWorker
import androidx.work.BackoffPolicy
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingWorkPolicy
import androidx.work.ForegroundInfo
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkRequest
import androidx.work.WorkerParameters
import dagger.assisted.Assisted
import dagger.assisted.AssistedInject
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.toWorkerResult
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import timber.log.Timber
import java.util.concurrent.TimeUnit
import io.paritytech.polkadotapp.common.R as RCommon

/**
 * Keeps the core's durable recovery running while it has transactions
 * awaiting a verdict. The core asks for it through
 * `HostBridge.durableWorkChanged`; the run returns once nothing is live.
 *
 * Distinct from the native coinage engine's `DurableTxRecovery` worker: the
 * two ledgers share nothing. It runs in the foreground because a transaction
 * can stay undecided for a whole era, past WorkManager's background limit.
 */
@HiltWorker
class CoreDurableRecoveryWorker @AssistedInject constructor(
    @Assisted appContext: Context,
    @Assisted params: WorkerParameters,
    private val runtimeProvider: TrUAPIHostRuntimeProvider,
) : CoroutineWorker(appContext, params) {
    companion object {
        private const val WORK_ID = "CoreDurableRecovery"

        private val NOTIFICATION_ID = WORK_ID.hashCode()

        /**
         * `APPEND_OR_REPLACE` queues a new run behind one that is finishing, so
         * a request made while a run settles is not dropped.
         */
        fun enqueue(context: Context) {
            val request = OneTimeWorkRequestBuilder<CoreDurableRecoveryWorker>()
                .setConstraints(
                    Constraints.Builder()
                        .setRequiredNetworkType(NetworkType.CONNECTED)
                        .build()
                )
                .setBackoffCriteria(
                    BackoffPolicy.LINEAR,
                    WorkRequest.MIN_BACKOFF_MILLIS,
                    TimeUnit.MILLISECONDS
                )
                .build()

            WorkManager.getInstance(context)
                .enqueueUniqueWork(WORK_ID, ExistingWorkPolicy.APPEND_OR_REPLACE, request)
        }
    }

    override suspend fun doWork(): Result {
        promoteToForeground()

        return runtimeProvider.runtime()
            .mapCatching { runtime -> runtime.runDurableRecovery() }
            .logFailure("Core durable recovery stopped early")
            .toWorkerResult(retryOnFailure = true)
    }

    /** A refused promotion only leaves the run under the background limit; the retry picks up the rest. */
    private suspend fun promoteToForeground() {
        runCatching { setForeground(getForegroundInfo()) }
            .onFailure { Timber.w(it, "Core durable recovery could not run in the foreground") }
    }

    override suspend fun getForegroundInfo(): ForegroundInfo {
        val title = applicationContext.getString(RCommon.string.core_recovery_worker_notification_title)
        val message = applicationContext.getString(RCommon.string.core_recovery_worker_notification_message)

        val notification = NotificationCompat.Builder(applicationContext, createChannel())
            .setContentTitle(title)
            .setSmallIcon(RCommon.drawable.ic_upgrade)
            .setTicker(title)
            .setContentText(message)
            .setOngoing(true)
            .build()

        return ForegroundInfo(NOTIFICATION_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
    }

    private fun createChannel(): String {
        val channelId = applicationContext.getString(RCommon.string.workers_notification_channel_id)

        val channel = NotificationChannel(
            channelId,
            applicationContext.getString(RCommon.string.workers_notification_channel_name),
            NotificationManager.IMPORTANCE_LOW
        )
        val notificationManager =
            applicationContext.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        notificationManager.createNotificationChannel(channel)

        return channelId
    }
}
