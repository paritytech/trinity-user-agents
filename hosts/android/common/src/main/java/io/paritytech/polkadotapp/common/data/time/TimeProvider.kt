package io.paritytech.polkadotapp.common.data.time

import javax.inject.Inject
import kotlin.time.Clock
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

@OptIn(ExperimentalTime::class)
interface TimeProvider {
    fun now(): Instant
}

@OptIn(ExperimentalTime::class)
class RealTimeProvider @Inject constructor() : TimeProvider {
    override fun now(): Instant = Clock.System.now()
}
