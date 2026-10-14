package io.paritytech.polkadotapp.feature_connection_status_impl.domain.health

import io.paritytech.polkadotapp.common.utils.currentTimestampFlow
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.launch
import kotlin.time.Duration

// DROP_OLDEST rather than the default SUSPEND, so the slowest probe cannot hold up the cadence of
// the others sharing this one timer; the replay lets a probe subscribing late evaluate immediately.
internal fun sharedSampleTicks(scope: CoroutineScope, period: Duration): Flow<Unit> {
    val ticks = MutableSharedFlow<Unit>(
        replay = 1,
        extraBufferCapacity = 1,
        onBufferOverflow = BufferOverflow.DROP_OLDEST,
    )
    scope.launch { currentTimestampFlow(period).collect { ticks.emit(Unit) } }
    return ticks
}
