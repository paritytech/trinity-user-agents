package io.paritytech.polkadotapp.feature_transactions_impl.data.tracked

import android.app.NotificationChannel
import android.app.NotificationManager
import android.content.Context
import androidx.core.app.NotificationCompat
import androidx.hilt.work.HiltWorker
import androidx.work.CoroutineWorker
import androidx.work.ForegroundInfo
import androidx.work.WorkerParameters
import dagger.assisted.Assisted
import dagger.assisted.AssistedInject
import io.novasama.substrate_sdk_android.runtime.definitions.types.fromHex
import io.novasama.substrate_sdk_android.runtime.definitions.types.generics.Extrinsic
import io.novasama.substrate_sdk_android.runtime.extrinsic.signer.SendableExtrinsic
import io.paritytech.polkadotapp.chains.extrinsic.ExtrinsicStatus
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.connection.ChainConnectionRefCounter
import io.paritytech.polkadotapp.chains.multiNetwork.connection.EnabledChainConnectionReference
import io.paritytech.polkadotapp.chains.multiNetwork.connection.requestConnectionEnabled
import io.paritytech.polkadotapp.chains.multiNetwork.getRuntime
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.ChainEventsRepositoryFactory
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.findExtrinsicFailureOrThrow
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.getExtrinsicWithEvents
import io.paritytech.polkadotapp.chains.multiNetwork.runtime.repository.isSuccess
import io.paritytech.polkadotapp.common.utils.mapAsync
import io.paritytech.polkadotapp.common.utils.mapToSet
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.common.utils.takeWhileInclusive
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import io.paritytech.polkadotapp.feature_transactions.api.data.bindDispatchError
import io.paritytech.polkadotapp.feature_transactions.api.data.retry.ResubmitWhenValidFactory
import io.paritytech.polkadotapp.feature_transactions.api.data.tracked.ExtrinsicTag
import io.paritytech.polkadotapp.feature_transactions.api.data.tracked.TrackedExtrinsicStatus
import io.paritytech.polkadotapp.feature_transactions_impl.data.retry.CauseBasedRecoveryStrategy
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.map
import timber.log.Timber
import io.paritytech.polkadotapp.common.R as RCommon

/**
 * Watches every non-terminal tracked extrinsic through to a terminal status, surviving process death.
 * On each sweep it reconstructs the persisted signed bytes into a [SendableExtrinsic] and drives the existing
 * watch-and-recover path; fork-induced invalidations resubmit the same bytes via [CauseBasedRecoveryStrategy] +
 * `ResubmitWhenValid`, while genuine rejections fail fast. A connection is held per chain for as long as that
 * chain has in-flight rows, including chains discovered on later sweeps; all are released when work ends.
 */
@HiltWorker
class TrackedExtrinsicWorker @AssistedInject constructor(
    @Assisted appContext: Context,
    @Assisted parameters: WorkerParameters,
    private val extrinsicService: ExtrinsicService,
    private val repository: TrackedExtrinsicRepository,
    private val chainRegistry: ChainRegistry,
    private val chainConnectionRefCounter: ChainConnectionRefCounter,
    private val resubmitWhenValidFactory: ResubmitWhenValidFactory,
    private val chainEventsRepositoryFactory: ChainEventsRepositoryFactory,
) : CoroutineWorker(appContext, parameters) {
    companion object {
        const val WORK_ID = "TrackedExtrinsic"
        private const val LABEL = "TrackedExtrinsicWorker"
        private val NOTIFICATION_ID = WORK_ID.hashCode()
    }

    private val notificationManager = appContext.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager

    private val heldConnections = mutableMapOf<ChainId, EnabledChainConnectionReference>()

    override suspend fun doWork(): Result {
        return try {
            watchUntilSettled()
        } finally {
            releaseConnections()
        }
    }

    private suspend fun watchUntilSettled(): Result {
        while (true) {
            val rows = repository.getUnfinished()
            if (rows.isEmpty()) {
                Timber.d("No tracked extrinsics left to watch; worker finishing")
                return Result.success()
            }

            Timber.d("Watching ${rows.size} tracked extrinsic(s): ${rows.map { it.tag }}")

            // Acquire a connection for any chain not already held — covers rows that land on later sweeps.
            ensureConnections(rows.mapToSet(TrackedExtrinsic::chainId))

            rows.mapAsync(::watchRow)
        }
    }

    private suspend fun ensureConnections(chainIds: Set<ChainId>) {
        chainIds.forEach { chainId ->
            if (chainId !in heldConnections) {
                heldConnections[chainId] = chainConnectionRefCounter.requestConnectionEnabled(chainId, LABEL)
            }
        }
    }

    private suspend fun releaseConnections() {
        heldConnections.values.forEach { it.release() }
        heldConnections.clear()
    }

    private suspend fun watchRow(row: TrackedExtrinsic) {
        runCancellableCatching {
            val chain = chainRegistry.getChain(row.chainId)
            val runtime = chainRegistry.getRuntime(row.chainId)
            val extrinsic = SendableExtrinsic(runtime, Extrinsic.fromHex(runtime, row.signedExtrinsic))

            val recovery = CauseBasedRecoveryStrategy(resubmitWhenValidFactory.create(row.chainId, maxAttempts = null))

            extrinsicService.submitAndWatchBuiltExtrinsic(chain, extrinsic, recovery)
                .map { status -> TrackedExtrinsicStatusReducer.reduce(status, inBlockDispatchError(row.chainId, status)) }
                .filterNotNull()
                .takeWhileInclusive { !it.terminal }
                .collect { tracked ->
                    repository.updateStatus(row.tag, tracked)
                    logStatus(row.tag, tracked)
                }
        }.onFailure { Timber.w(it, "Failed to watch tracked extrinsic ${row.tag}") }
    }

    private fun logStatus(tag: ExtrinsicTag, status: TrackedExtrinsicStatus) {
        when (status) {
            is TrackedExtrinsicStatus.Failed -> Timber.w("Tracked extrinsic $tag failed: ${status.message}")
            is TrackedExtrinsicStatus.Finalized -> Timber.i("Tracked extrinsic $tag finalized in block ${status.blockHash}")
            else -> Timber.d("Tracked extrinsic $tag -> $status")
        }
    }

    // Inclusion success ≠ dispatch success: on InBlock resolve the events and, if the call failed, return the
    // bound dispatch error so it is persisted in the Failed state. Returns null when the call succeeded or the
    // outcome can't be resolved (assume success rather than falsely failing a healthy tx).
    private suspend fun inBlockDispatchError(chainId: ChainId, status: ExtrinsicStatus): String? {
        if (status !is ExtrinsicStatus.InBlock) return null

        return runCancellableCatching {
            val repository = chainEventsRepositoryFactory.create(chainId)
            val withEvents = repository.getExtrinsicWithEvents(status.extrinsicHash, status.blockHash)
            if (withEvents == null || withEvents.isSuccess()) return@runCancellableCatching null

            val runtime = chainRegistry.getRuntime(chainId)
            val failureEvent = withEvents.events.findExtrinsicFailureOrThrow()
            bindDispatchError(failureEvent.arguments.first(), runtime).toString()
        }.onFailure {
            Timber.w(it, "Could not resolve in-block dispatch outcome for ${status.extrinsicHash}; treating as success")
        }.getOrNull()
    }

    override suspend fun getForegroundInfo(): ForegroundInfo {
        val channelId = createChannel()

        val notification = NotificationCompat.Builder(applicationContext, channelId)
            .setContentTitle(applicationContext.getString(RCommon.string.tracked_extrinsic_worker_notification_title))
            .setSmallIcon(RCommon.drawable.ic_upgrade)
            .setContentText(applicationContext.getString(RCommon.string.tracked_extrinsic_worker_notification_message))
            .setOngoing(true)
            .build()

        return ForegroundInfo(NOTIFICATION_ID, notification)
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
