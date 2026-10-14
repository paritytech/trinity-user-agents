package io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment

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
import androidx.work.OutOfQuotaPolicy
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import dagger.assisted.Assisted
import dagger.assisted.AssistedInject
import io.paritytech.polkadotapp.chains.multiNetwork.connection.ChainConnectionRefCounter
import io.paritytech.polkadotapp.chains.multiNetwork.connection.withConnectionEnabled
import io.paritytech.polkadotapp.common.data.worker.stateMachine.executeUntilPossible
import io.paritytech.polkadotapp.common.data.worker.stateMachine.toWorkerResult
import io.paritytech.polkadotapp.feature_coinage_impl.domain.coinageLogD
import io.paritytech.polkadotapp.feature_coinage_impl.domain.externalPayment.repository.ExternalPaymentRepository
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import java.util.concurrent.TimeUnit
import io.paritytech.polkadotapp.common.R as RCommon

@HiltWorker
class ExternalPaymentWorker @AssistedInject constructor(
    @Assisted appContext: Context,
    @Assisted parameters: WorkerParameters,
    private val repository: ExternalPaymentRepository,
    private val stateMachineFactory: ExternalPaymentStateMachine.Factory,
    private val chainConnectionRefCounter: ChainConnectionRefCounter,
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider
) : CoroutineWorker(appContext, parameters) {
    companion object {
        private const val WORK_ID = "ExternalPayment"
        private val NOTIFICATION_ID = WORK_ID.hashCode()

        fun start(context: Context) {
            val request = OneTimeWorkRequestBuilder<ExternalPaymentWorker>()
                .setExpedited(OutOfQuotaPolicy.RUN_AS_NON_EXPEDITED_WORK_REQUEST)
                .setBackoffCriteria(BackoffPolicy.LINEAR, 30, TimeUnit.SECONDS)
                .setConstraints(
                    Constraints.Builder()
                        .setRequiredNetworkType(NetworkType.CONNECTED)
                        .build()
                )
                .build()

            WorkManager.getInstance(context)
                .enqueueUniqueWork(WORK_ID, ExistingWorkPolicy.KEEP, request)
        }
    }

    private val notificationManager = appContext.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager

    override suspend fun doWork(): Result {
        coinageLogD("External payment worker run started")

        return chainConnectionRefCounter.withConnectionEnabled(chainAssetProvider.chainId(), "ExternalPaymentWorker") {
            processExternalPayments()
        }.also { coinageLogD("External payment worker run finished result=$it") }
    }

    private suspend fun processExternalPayments(): Result {
        while (true) {
            val pending = repository.getNextPending() ?: return Result.success()

            coinageLogD("External payment processing payment=${pending.key} stage=${pending.stage}")

            val outcome = stateMachineFactory.create(pending.key)
                .executeUntilPossible()
                .toWorkerResult(retryOnFailure = true)

            if (outcome !is Result.Success) return outcome
        }
    }

    override suspend fun getForegroundInfo(): ForegroundInfo {
        val title = applicationContext.getString(RCommon.string.external_payment_worker_notification_title)
        val message = applicationContext.getString(RCommon.string.external_payment_worker_notification_message)

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

        notificationManager.createNotificationChannel(channel)

        return channelId
    }
}
