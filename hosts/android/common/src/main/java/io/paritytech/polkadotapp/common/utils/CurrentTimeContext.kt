package io.paritytech.polkadotapp.common.utils

import kotlin.time.Clock
import kotlin.time.Instant

/**
 * Centralised access to the current wall-clock instant.
 *
 * Use [currentTime] instead of [Clock.System.now] directly so that production code is consistent
 * and tests can substitute the implementation via a fake.
 */
object CurrentTimeContext {
    fun currentTime(): Instant = Clock.System.now()
}
