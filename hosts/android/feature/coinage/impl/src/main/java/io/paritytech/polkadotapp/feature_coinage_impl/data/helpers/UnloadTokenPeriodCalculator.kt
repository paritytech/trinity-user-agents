package io.paritytech.polkadotapp.feature_coinage_impl.data.helpers

import javax.inject.Inject
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds

interface UnloadTokenPeriodCalculator {
    fun currentPeriod(periodDuration: Duration): Long
}

class RealUnloadTokenPeriodCalculator @Inject constructor() : UnloadTokenPeriodCalculator {
    override fun currentPeriod(periodDuration: Duration): Long {
        val currentTime = System.currentTimeMillis()

        val now = currentTime.milliseconds.coerceAtLeast(Duration.ZERO)

        return now.inWholeSeconds / periodDuration.inWholeSeconds
    }
}
