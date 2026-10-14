package io.paritytech.polkadotapp.test_shared

import io.paritytech.polkadotapp.common.data.time.TimeProvider
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

@OptIn(ExperimentalTime::class)
class FakeTimeProvider(private val nowMillis: () -> Long) : TimeProvider {
    override fun now(): Instant = Instant.fromEpochMilliseconds(nowMillis())
}
