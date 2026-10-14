package io.paritytech.polkadotapp.feature_statement_store_api.domain.models

private const val U32_MAX: ULong = 0xFFFF_FFFFuL

object StatementExpiry {
    fun createForCurrentTimestamp(): ULong {
        val currentSeconds = StatementTimestamp.currentEpochSeconds()
        return createWithPriority(currentSeconds.toUInt())
    }

    fun createDefault(): ULong {
        return createWithPriority(0u)
    }

    fun createWithPriority(priority: UInt): ULong {
        return (U32_MAX shl 32) or priority.toULong()
    }

    /** Monotonic successor to [prev], floored at the current wall-clock priority (mirrors iOS `incrementedExpiry`). */
    fun nextAfter(prev: ULong): ULong = maxOf(prev + 1uL, createForCurrentTimestamp())
}
