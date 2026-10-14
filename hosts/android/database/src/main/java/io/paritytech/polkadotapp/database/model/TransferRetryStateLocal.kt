package io.paritytech.polkadotapp.database.model

import androidx.room.ColumnInfo

class TransferRetryStateLocal(
    @ColumnInfo(defaultValue = "0")
    val attemptCount: Int,
    val firstFailureAt: Long?,
    val nextAttemptAt: Long?
) {
    companion object {
        val None = TransferRetryStateLocal(attemptCount = 0, firstFailureAt = null, nextAttemptAt = null)
    }
}
