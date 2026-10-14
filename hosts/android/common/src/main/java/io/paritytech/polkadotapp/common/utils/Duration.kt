package io.paritytech.polkadotapp.common.utils

import kotlin.time.Duration
import kotlin.time.DurationUnit

inline val Duration.lastDays: Long
    get() = this.inWholeDays

val Duration.lastHours: Int
    get() = this.toComponents { _, hours, _, _, _ -> hours }

val Duration.lastMinutes: Int
    get() = this.toComponents { _, _, minutes, _, _ -> minutes }

val Duration.inFractionalSeconds: Double
    get() = toDouble(DurationUnit.SECONDS)

fun Duration?.orZero(): Duration = this ?: Duration.ZERO

fun calculateProgress(totalDuration: Duration, timeLeft: Duration): Float {
    val millisTotal = totalDuration.inWholeMilliseconds

    if (millisTotal <= 0) {
        return 0f
    }

    val millisLeft = timeLeft.inWholeMilliseconds
    val elapsed = millisTotal - millisLeft

    val progress = elapsed.toFloat() / millisTotal.toFloat()

    return progress.coerceIn(0f, 1f)
}
