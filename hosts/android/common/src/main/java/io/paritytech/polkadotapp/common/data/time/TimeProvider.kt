package io.paritytech.polkadotapp.common.data.time

import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow
import javax.inject.Inject
import kotlin.time.Clock
import kotlin.time.Duration
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

@OptIn(ExperimentalTime::class)
interface TimeProvider {
    fun now(): Instant

    /** The time now, then again every [period] while collected. */
    fun ticking(period: Duration): Flow<Instant> = flow {
        while (true) {
            emit(now())
            delay(period)
        }
    }
}

@OptIn(ExperimentalTime::class)
class RealTimeProvider @Inject constructor() : TimeProvider {
    override fun now(): Instant = Clock.System.now()
}
