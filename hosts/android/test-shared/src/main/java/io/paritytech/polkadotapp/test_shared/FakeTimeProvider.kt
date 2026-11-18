package io.paritytech.polkadotapp.test_shared

import io.paritytech.polkadotapp.common.data.time.TimeProvider
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flowOf
import kotlin.time.Duration
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

@OptIn(ExperimentalTime::class)
class FakeTimeProvider(private val nowMillis: () -> Long) : TimeProvider {
    override fun now(): Instant = Instant.fromEpochMilliseconds(nowMillis())

    // One reading, so a test's virtual clock never runs a ticker forever.
    override fun ticking(period: Duration): Flow<Instant> = flowOf(now())
}
