@file:OptIn(ExperimentalTime::class)

package io.paritytech.polkadotapp.feature_statement_store_impl.domain.slotAllocator

import javax.inject.Inject
import kotlin.time.Clock
import kotlin.time.Duration.Companion.days
import kotlin.time.ExperimentalTime

internal val PERIOD_DURATION_SECONDS: Long = 1.days.inWholeSeconds

/**
 * Single source of "what period is it right now" for the slot allocator and renewer.
 * Extracted so tests can inject a fake instead of stubbing [Clock.System] statically.
 */
fun interface CurrentPeriodProvider {
    fun current(): UInt
}

class RealCurrentPeriodProvider @Inject constructor() : CurrentPeriodProvider {
    override fun current(): UInt =
        (Clock.System.now().epochSeconds / PERIOD_DURATION_SECONDS).toUInt()
}
