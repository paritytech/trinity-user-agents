package io.paritytech.polkadotapp.feature_statement_store_api.domain.models

/**
 * Utility object for working with statement store timestamps.
 * All timestamps in the statement store protocol are relative to a protocol epoch.
 */
object StatementTimestamp {
    /**
     * Protocol epoch timestamp in seconds (Unix time).
     * All statement store timestamps are relative to this epoch.
     */
    private const val PROTOCOL_EPOCH_SECONDS = 1763164800L

    private const val SECONDS_IN_DAY = 86_400L

    /**
     * Returns the current time as seconds since the protocol epoch.
     */
    fun currentEpochSeconds(): Long {
        val currentUnixSeconds = System.currentTimeMillis() / 1000
        // Drift-back clock guard (device set before PROTOCOL_EPOCH): a negative value here
        // would `.toUInt()` into a near-U32_MAX priority and balloon expiry to ~u64::MAX,
        // causing the server to reject submits with ChannelPriorityTooLow.
        return (currentUnixSeconds - PROTOCOL_EPOCH_SECONDS).coerceAtLeast(0)
    }

    /**
     * Converts Unix timestamp (seconds) to epoch-relative seconds.
     */
    fun toEpochSeconds(unixTimestampSeconds: Long): Long {
        return unixTimestampSeconds - PROTOCOL_EPOCH_SECONDS
    }

    /**
     * Calculates the day number from Unix timestamp (seconds).
     * Day number is the number of days since the protocol epoch.
     */
    fun calculateDay(unixTimestampSeconds: Long): Long {
        return toEpochSeconds(unixTimestampSeconds) / SECONDS_IN_DAY
    }

    /**
     * Returns the current day number (days since protocol epoch).
     */
    fun currentDay(): Long {
        return currentEpochSeconds() / SECONDS_IN_DAY
    }
}
