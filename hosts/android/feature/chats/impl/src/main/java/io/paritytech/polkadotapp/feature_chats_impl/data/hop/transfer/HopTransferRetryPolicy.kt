package io.paritytech.polkadotapp.feature_chats_impl.data.hop.transfer

import io.paritytech.polkadotapp.feature_chats_impl.domain.hop.HopTransferRetryState
import javax.inject.Inject
import kotlin.time.Duration
import kotlin.time.Duration.Companion.hours
import kotlin.time.Duration.Companion.seconds
import kotlin.time.Instant

sealed interface RetryDecision {
    data class Reschedule(
        val attemptCount: Int,
        val firstFailureAt: Instant,
        val nextAttemptAt: Instant
    ) : RetryDecision

    data object Terminal : RetryDecision

    data object Ignore : RetryDecision
}

class HopTransferRetryPolicy @Inject constructor() {
    fun decide(current: HopTransferRetryState, error: Throwable, now: Instant): RetryDecision {
        if (error is TransferCancelledException) return RetryDecision.Ignore
        if (error is TerminalTransferException) return RetryDecision.Terminal

        val firstFailureAt = current.firstFailureAt ?: now
        if (now - firstFailureAt >= RETRY_WINDOW) return RetryDecision.Terminal

        val attemptCount = current.attemptCount + 1

        return RetryDecision.Reschedule(
            attemptCount = attemptCount,
            firstFailureAt = firstFailureAt,
            nextAttemptAt = now + backoff(attemptCount)
        )
    }

    private fun backoff(attemptCount: Int): Duration {
        val exponent = (attemptCount - 1).coerceIn(0, MAX_BACKOFF_EXPONENT)
        return (BASE_BACKOFF * (1L shl exponent).toDouble()).coerceAtMost(MAX_BACKOFF)
    }

    private companion object {
        val BASE_BACKOFF = 10.seconds
        val MAX_BACKOFF = 1.hours
        val RETRY_WINDOW = 24.hours
        const val MAX_BACKOFF_EXPONENT = 20
    }
}
