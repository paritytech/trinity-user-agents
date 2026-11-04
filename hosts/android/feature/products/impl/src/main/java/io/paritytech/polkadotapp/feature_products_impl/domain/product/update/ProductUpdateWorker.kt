package io.paritytech.polkadotapp.feature_products_impl.domain.product.update

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
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.connection.ChainConnectionRefCounter
import io.paritytech.polkadotapp.chains.multiNetwork.connection.withConnectionEnabled
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.toWorkerResult
import java.util.concurrent.TimeUnit
import kotlin.time.Duration.Companion.hours

@HiltWorker
class ProductUpdateWorker @AssistedInject constructor(
    @Assisted appContext: Context,
    @Assisted params: WorkerParameters,
    private val productUpdater: ProductUpdater,
    private val chainRegistry: ChainRegistry,
    private val chainConnectionRefCounter: ChainConnectionRefCounter,
) : CoroutineWorker(appContext, params) {
    override suspend fun doWork(): Result {
        return chainConnectionRefCounter.withConnectionEnabled(chainRegistry.knownChains.assetHub, WORK_NAME) {
            productUpdater.checkAll()
        }
            .logFailure("Product update check failed")
            .toWorkerResult(retryOnFailure = true)
    }

    companion object {
        private const val WORK_NAME = "ProductUpdates"

        private val INTERVAL = 4.hours

        fun schedule(context: Context) {
            val request = PeriodicWorkRequestBuilder<ProductUpdateWorker>(INTERVAL.inWholeSeconds, TimeUnit.SECONDS)
                .setConstraints(
                    Constraints.Builder()
                        .setRequiredNetworkType(NetworkType.UNMETERED)
                        .setRequiresBatteryNotLow(true)
                        .build()
                )
                .setBackoffCriteria(BackoffPolicy.EXPONENTIAL, 30, TimeUnit.SECONDS)
                .build()

            WorkManager.getInstance(context)
                .enqueueUniquePeriodicWork(WORK_NAME, ExistingPeriodicWorkPolicy.KEEP, request)
        }
    }
}
