package io.paritytech.polkadotapp.feature_coinage_api.domain.model

/**
 * One step a coin has taken since it left a recycler.
 *
 * Each hop costs the coin some of the anonymity it came out with, and how much depends on how many other
 * coins moved with it — a transfer of one coin on its own hides in nothing, a transfer of eight hides in
 * eight. The size of that crowd is what each variant carries.
 *
 * A crowd size is stored as an unsigned 8-bit count and every call site derives it from a list length, so
 * the factories coerce rather than validate: a crowd of one is the smallest honest answer, and a crowd
 * larger than the range can hold is indistinguishable from the largest it can. Constructors are private so
 * that coercion cannot be stepped around.
 */
sealed interface Hop {
    /** [bundleSize] coins moved together in the transfer this hop records. */
    data class Transfer private constructor(val bundleSize: Int) : Hop {
        companion object {
            fun of(bundleSize: Int) = Transfer(bundleSize.coerceIn(MIN_CROWD, MAX_CROWD))
        }
    }

    /** [fanout] coins came out of the split this hop records, counting change. */
    data class Split private constructor(val fanout: Int) : Hop {
        companion object {
            fun of(fanout: Int) = Split(fanout.coerceIn(MIN_CROWD, MAX_CROWD))
        }
    }
}

fun transferHop(bundleSize: Int) = Hop.Transfer.of(bundleSize)

fun splitHop(fanout: Int) = Hop.Split.of(fanout)

/**
 * Top-level rather than in a companion: reading a private companion const from a value-class or nested
 * companion member crashes the Kotlin backend the same way it does for enums (see `RingExponent`).
 */
private const val MIN_CROWD = 1
private const val MAX_CROWD = 255
