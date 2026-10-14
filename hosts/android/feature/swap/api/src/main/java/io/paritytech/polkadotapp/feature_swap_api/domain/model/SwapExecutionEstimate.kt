package io.paritytech.polkadotapp.feature_swap_api.domain.model

import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.common.domain.model.toTimestamp
import io.paritytech.polkadotapp.common.utils.sum
import kotlin.time.Duration

class SwapExecutionEstimate(
    private val atomicOperationsEstimates: List<Duration>,
    private val additionalBuffer: Duration
) {
    fun totalTime(): Duration {
        return remainingTimeWhenExecuting(stepIndex = 0)
    }

    fun completesAtFromScratch(): Timestamp {
        return completesAtWhenExecuting(stepIndex = 0)
    }

    fun completesAtWhenExecuting(stepIndex: Int): Timestamp {
        return System.currentTimeMillis() + remainingTimeWhenExecuting(stepIndex).toTimestamp()
    }

    fun remainingTimeWhenExecuting(stepIndex: Int): Duration {
        return atomicOperationsEstimates.drop(stepIndex).sum() + additionalBuffer
    }
}
