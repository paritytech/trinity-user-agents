package io.paritytech.polkadotapp.feature_statement_store_impl.domain.slotAllocator

import android.content.Context
import androidx.hilt.work.HiltWorker
import androidx.work.BackoffPolicy
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.NetworkType
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import dagger.assisted.Assisted
import dagger.assisted.AssistedInject
import io.paritytech.polkadotapp.chains.multiNetwork.connection.ChainConnectionRefCounter
import io.paritytech.polkadotapp.chains.multiNetwork.connection.withConnectionEnabled
import io.paritytech.polkadotapp.common.utils.toWorkerResult
import java.util.concurrent.TimeUnit
import kotlin.time.Duration.Companion.hours

@HiltWorker
class StatementStoreSlotRenewalWorker @AssistedInject constructor(
    @Assisted appContext: Context,
    @Assisted parameters: WorkerParameters,
    private val renewer: StatementStoreSlotRenewer,
    private val contextResolver: AllocateContextResolver,
    private val renewalLock: StatementStoreSlotRenewalLock,
    private val chainConnectionRefCounter: ChainConnectionRefCounter,
) : CoroutineWorker(appContext, parameters) {
    override suspend fun doWork(): Result {
        val outcome = contextResolver.resolve().mapCatching { context ->
            chainConnectionRefCounter.withConnectionEnabled(context.chain.id, WORK_NAME) {
                renewalLock.withLock {
                    renewer.renew(context, priorityAccount = null).getOrThrow()
                }
            }
        }
        return outcome.toWorkerResult(retryOnFailure = true)
    }

    companion object {
        const val WORK_NAME = "StatementStoreSlotRenewal"

        val PERIOD_EXPIRATION_GRACE_PERIOD = 1.hours

        fun schedule(context: Context) {
            // Run with the same frequency as period expiration grace period to ensure we are able to renew before grace period ends
            val intervalSeconds = PERIOD_EXPIRATION_GRACE_PERIOD.inWholeSeconds

            val request = PeriodicWorkRequestBuilder<StatementStoreSlotRenewalWorker>(
                repeatInterval = intervalSeconds,
                repeatIntervalTimeUnit = TimeUnit.SECONDS,
            )
                .setBackoffCriteria(BackoffPolicy.EXPONENTIAL, 30, TimeUnit.SECONDS)
                .setConstraints(
                    Constraints.Builder()
                        .setRequiredNetworkType(NetworkType.CONNECTED)
                        .build()
                )
                .build()

            WorkManager.getInstance(context)
                .enqueueUniquePeriodicWork(WORK_NAME, ExistingPeriodicWorkPolicy.KEEP, request)
        }
    }
}
