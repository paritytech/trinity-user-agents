package io.paritytech.polkadotapp.feature_connection_status_impl.domain.health.probe

import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.feature_connection_status_api.domain.model.ChainMetricReading
import io.paritytech.polkadotapp.feature_connection_status_impl.domain.health.PendingRequestTracker
import io.paritytech.polkadotapp.feature_connection_status_impl.domain.health.scoring.ChainHealthThresholds
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChangedBy
import javax.inject.Inject
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.ExperimentalTime

@OptIn(ExperimentalTime::class)
class UnansweredRequestProbe @Inject constructor(
    private val timeProvider: TimeProvider,
) : ChainHealthProbe {
    override fun observe(context: ChainMetricContext): Flow<ChainMetricReading> {
        val tracker = PendingRequestTracker()
        val limit = context.expectedBlockTime * ChainHealthThresholds.UNANSWERED_REQUEST_BLOCK_TIMES

        return combine(context.pendingRequests, context.ticks) { pending, _ ->
            ChainMetricReading.UnansweredRequest(
                age = tracker.update(pending, timeProvider.now().toEpochMilliseconds()).milliseconds,
                limit = limit,
            )
        }
            .distinctUntilChangedBy { it.isOverLimit }
    }
}
