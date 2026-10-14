package io.paritytech.polkadotapp.feature_chats_impl.domain.hop

import kotlin.time.Instant

data class HopTransferRetryState(
    val attemptCount: Int,
    val firstFailureAt: Instant?,
    val nextAttemptAt: Instant?
) {
    companion object {
        val None = HopTransferRetryState(attemptCount = 0, firstFailureAt = null, nextAttemptAt = null)
    }
}
