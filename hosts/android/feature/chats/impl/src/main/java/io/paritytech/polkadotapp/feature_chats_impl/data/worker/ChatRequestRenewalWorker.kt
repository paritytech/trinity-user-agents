package io.paritytech.polkadotapp.feature_chats_impl.data.worker

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
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.connection.ChainConnectionRefCounter
import io.paritytech.polkadotapp.chains.multiNetwork.connection.withConnectionEnabled
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.common.utils.toWorkerResult
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.renewal.ChatRequestRenewer
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTransactionService
import timber.log.Timber
import java.util.concurrent.TimeUnit
import kotlin.time.Duration.Companion.hours

@HiltWorker
class ChatRequestRenewalWorker @AssistedInject constructor(
    @Assisted appContext: Context,
    @Assisted parameters: WorkerParameters,
    private val renewer: ChatRequestRenewer,
    private val durableTransactionService: DurableTransactionService,
    private val chainConnectionRefCounter: ChainConnectionRefCounter,
    private val knownChains: KnownChains,
) : CoroutineWorker(appContext, parameters) {
    override suspend fun doWork(): Result {
        val outcome = runCancellableCatching {
            chainConnectionRefCounter.withConnectionEnabled(knownChains.people, WORK_NAME) {
                // Slot claims are built by the durable executor, which a worker started after process death must start.
                durableTransactionService.startRecovery()
                Timber.i("chatRequestRenewal: worker run started (attempt $runAttemptCount)")
                renewer.renew().getOrThrow()
            }
        }
        outcome.onFailure { Timber.e(it, "chatRequestRenewal: worker run failed") }
        return outcome.toWorkerResult(retryOnFailure = true)
    }

    companion object {
        const val WORK_NAME = "ChatRequestRenewal"

        private val RENEWAL_INTERVAL = 1.hours

        fun schedule(context: Context) {
            val request = PeriodicWorkRequestBuilder<ChatRequestRenewalWorker>(
                repeatInterval = RENEWAL_INTERVAL.inWholeSeconds,
                repeatIntervalTimeUnit = TimeUnit.SECONDS,
            )
                .setBackoffCriteria(BackoffPolicy.EXPONENTIAL, 30, TimeUnit.SECONDS)
                .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
                .build()

            WorkManager.getInstance(context)
                .enqueueUniquePeriodicWork(WORK_NAME, ExistingPeriodicWorkPolicy.KEEP, request)
        }
    }
}
