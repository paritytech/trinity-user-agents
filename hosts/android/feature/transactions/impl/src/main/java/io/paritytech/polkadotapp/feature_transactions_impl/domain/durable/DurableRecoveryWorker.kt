package io.paritytech.polkadotapp.feature_transactions_impl.domain.durable

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
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.chains.multiNetwork.connection.ChainConnectionRefCounter
import io.paritytech.polkadotapp.chains.multiNetwork.connection.withConnectionEnabled
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.toWorkerResult
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.TxCompletionOracle
import java.util.concurrent.TimeUnit
import javax.inject.Inject
import io.paritytech.polkadotapp.common.R as RCommon

/**
 * Hosts [DurableRecoveryLoop] for as long as any transaction is undecided.
 *
 * One-time rather than periodic: the loop is driven by heads and ends by itself once the ledger settles, so
 * a repeat interval would only add passes that could not learn anything. `KEEP` makes every caller's
 * [DurableRecoveryScheduler.ensureRunning] idempotent against the running loop.
 *
 * It runs in the foreground because a transaction can stay undecided for a whole mortality window, well
 * past WorkManager's execution limit for a background worker — and letting a lock outlive the process that
 * could release it is the failure this subsystem exists to prevent.
 */
@HiltWorker
class DurableRecoveryWorker @AssistedInject constructor(
    @Assisted appContext: Context,
    @Assisted params: WorkerParameters,
    private val recoveryLoop: DurableRecoveryLoop,
    private val submissionExecutor: DurableSubmissionExecutor,
    private val chainConnectionRefCounter: ChainConnectionRefCounter,
    private val oracles: Map<String, @JvmSuppressWildcards TxCompletionOracle>,
) : CoroutineWorker(appContext, params) {
    companion object {
        private const val WORK_ID = "DurableTxRecovery"

        private val NOTIFICATION_ID = WORK_ID.hashCode()

        fun enqueue(context: Context) {
            val request = OneTimeWorkRequestBuilder<DurableRecoveryWorker>()
                .setConstraints(
                    Constraints.Builder()
                        .setRequiredNetworkType(NetworkType.CONNECTED)
                        .build()
                )
                // Connectivity returning is a trigger in its own right: a loop that dropped its
                // subscription fails, and the constraint above holds the retry until there is a network.
                .setBackoffCriteria(
                    BackoffPolicy.LINEAR,
                    WorkRequest.MIN_BACKOFF_MILLIS,
                    TimeUnit.MILLISECONDS
                )
                .build()

            WorkManager.getInstance(context)
                .enqueueUniqueWork(WORK_ID, ExistingWorkPolicy.KEEP, request)
        }
    }

    override suspend fun doWork(): Result {
        durabilityLogD("Recovery worker run started")

        promoteToForeground()

        // A restart after process death reaches this worker without anything having started the executor, and
        // the loop would keep a transaction waiting to be built alive without anyone building it.
        submissionExecutor.ensureStarted()

        // A connection per chain a domain lives on: the loop watches all of them, and a pass can pin any.
        val chains = oracles.values.mapTo(mutableSetOf()) { it.chainId }

        return chainConnectionRefCounter.withConnectionEnabled(chains, WORK_ID) {
            recoveryLoop.runUntilSettled()
        }.logFailure("Durable recovery loop stopped early")
            .onSuccess { durabilityLogD("Recovery worker run settled") }
            .onFailure { durabilityLogW("Recovery worker run stopped early: $it") }
            .toWorkerResult(retryOnFailure = true)
    }

    /**
     * A refused promotion is survivable — the loop then runs under the ordinary execution limit and its
     * unfinished work is picked up by the next [enqueue] — so it must not take the whole worker down.
     */
    private suspend fun promoteToForeground() {
        runCatching { setForeground(getForegroundInfo()) }
            .onFailure { durabilityLogW("Durable recovery could not run in the foreground: $it") }
    }

    override suspend fun getForegroundInfo(): ForegroundInfo {
        val title = applicationContext.getString(RCommon.string.coinage_recovery_worker_notification_title)
        val message = applicationContext.getString(RCommon.string.coinage_recovery_worker_notification_message)

        val channelId = createChannel()

        val notification = NotificationCompat.Builder(applicationContext, channelId)
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

class WorkManagerDurableRecoveryScheduler @Inject constructor(
    @param:ApplicationContext private val context: Context,
) : DurableRecoveryScheduler {
    /**
     * Enqueueing is best-effort by contract: the caller has already committed a ledger row, and losing the
     * scheduler must never undo that. The next launch starts the loop anyway.
     */
    override fun ensureRunning() {
        runCatching { DurableRecoveryWorker.enqueue(context) }
            .onFailure { durabilityLogW("Could not enqueue durable recovery: $it") }
    }
}
