package io.paritytech.polkadotapp.feature_chats_impl.data.repository

import io.paritytech.polkadotapp.database.model.TransferRetryStateLocal
import io.paritytech.polkadotapp.feature_chats_impl.domain.hop.HopTransferRetryState
import kotlin.time.Instant

internal fun TransferRetryStateLocal.toDomain() = HopTransferRetryState(
    attemptCount = attemptCount,
    firstFailureAt = firstFailureAt?.let(Instant::fromEpochMilliseconds),
    nextAttemptAt = nextAttemptAt?.let(Instant::fromEpochMilliseconds)
)

internal fun HopTransferRetryState.toLocal() = TransferRetryStateLocal(
    attemptCount = attemptCount,
    firstFailureAt = firstFailureAt?.toEpochMilliseconds(),
    nextAttemptAt = nextAttemptAt?.toEpochMilliseconds()
)
