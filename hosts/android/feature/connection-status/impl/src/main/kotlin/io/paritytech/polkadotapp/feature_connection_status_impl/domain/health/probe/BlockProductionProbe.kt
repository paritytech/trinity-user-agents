package io.paritytech.polkadotapp.feature_connection_status_impl.domain.health.probe

import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.feature_connection_status_api.domain.model.ChainConnectionPresentation
import io.paritytech.polkadotapp.feature_connection_status_api.domain.model.ChainMetricReading
import io.paritytech.polkadotapp.feature_connection_status_impl.domain.health.BlockProductionAnchor
import io.paritytech.polkadotapp.feature_connection_status_impl.domain.health.BlockProductionAnchorSource
import io.paritytech.polkadotapp.feature_connection_status_impl.domain.health.BlockProductionWindow
import io.paritytech.polkadotapp.feature_connection_status_impl.domain.health.chainHealthLog
import io.paritytech.polkadotapp.feature_connection_status_impl.domain.health.scoring.ChainHealthThresholds
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.merge
import kotlinx.coroutines.withTimeoutOrNull
import javax.inject.Inject
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

@OptIn(ExperimentalTime::class, ExperimentalCoroutinesApi::class)
class BlockProductionProbe @Inject constructor(
    private val timeProvider: TimeProvider,
    private val anchorSource: BlockProductionAnchorSource,
) : ChainHealthProbe {
    override fun observe(context: ChainMetricContext): Flow<ChainMetricReading> {
        val blockTime = context.expectedBlockTime.coerceAtLeast(MIN_BLOCK_TIME)
        val window = maxOf(
            ChainHealthThresholds.BLOCK_PRODUCTION_MIN_WINDOW,
            blockTime * ChainHealthThresholds.BLOCK_PRODUCTION_MIN_SLOTS,
        )
        val expectedBlocks = (window / blockTime).toInt().coerceAtLeast(MIN_EXPECTED_BLOCKS)
        val unmeasured = ChainMetricReading.BlockProduction(producedBlocks = null, expectedBlocks = expectedBlocks, anchorPending = false)

        return context.connection
            .map { it == ChainConnectionPresentation.Connected }
            .distinctUntilChanged()
            .flatMapLatest { connected ->
                if (connected) measureWhileConnected(context, window, expectedBlocks) else flowOf(unmeasured)
            }
            .distinctUntilChanged()
    }

    private fun measureWhileConnected(
        context: ChainMetricContext,
        window: Duration,
        expectedBlocks: Int,
    ): Flow<ChainMetricReading.BlockProduction> = flow {
        chainHealthLog.d("%s connected: measuring %d expected blocks over a %s window", context.chainId, expectedBlocks, window)
        val production = BlockProductionWindow(window)
        var anchorPending = true

        val heads = context.bestBlockNumber.distinctUntilChanged().map(Event::Head)
        val ticks = context.ticks.map { Event.Tick }
        val anchored = flow { emit(Event.Anchored(anchor(context, expectedBlocks))) }

        emitAll(
            merge(heads, ticks, anchored).map { event ->
                val now = timeProvider.now()
                when (event) {
                    is Event.Head -> production.record(height = event.height, at = now)
                    is Event.Anchored -> {
                        event.anchor?.let { production.seed(it, now) }
                        anchorPending = false
                    }
                    Event.Tick -> Unit
                }
                reading(production, now, expectedBlocks, anchorPending)
            },
        )
    }

    private suspend fun anchor(context: ChainMetricContext, expectedBlocks: Int): BlockProductionAnchor? {
        val timeout = ChainHealthThresholds.BLOCK_PRODUCTION_ANCHOR_TIMEOUT
        val result = withTimeoutOrNull(timeout) { anchorSource.fetch(context.chainId, expectedBlocks) }

        if (result == null) {
            chainHealthLog.w("%s anchor: no answer within %s", context.chainId, timeout)
            return null
        }

        return result.fold(
            onSuccess = { anchor ->
                chainHealthLog.d("%s anchor: head %d, last %d blocks took %s", context.chainId, anchor.headHeight, anchor.blocks, anchor.chainClockSpan)
                anchor
            },
            onFailure = { failure ->
                chainHealthLog.w(failure, "%s anchor: unusable answer", context.chainId)
                null
            },
        )
    }

    private fun reading(
        production: BlockProductionWindow,
        now: Instant,
        expectedBlocks: Int,
        anchorPending: Boolean,
    ): ChainMetricReading.BlockProduction = ChainMetricReading.BlockProduction(
        producedBlocks = production.blocksProduced(now)?.coerceAtMost(expectedBlocks),
        expectedBlocks = expectedBlocks,
        anchorPending = anchorPending,
    )

    private sealed interface Event {
        data class Head(val height: Int) : Event
        data class Anchored(val anchor: BlockProductionAnchor?) : Event
        data object Tick : Event
    }

    private companion object {
        val MIN_BLOCK_TIME: Duration = 1.milliseconds
        const val MIN_EXPECTED_BLOCKS = 1
    }
}
