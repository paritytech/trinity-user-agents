package io.paritytech.polkadotapp.common.presentation.formatters.time.duration

import kotlin.time.Duration

internal interface DurationFormatter {
    fun format(duration: Duration): String
}

internal interface BoundedDurationFormatter : DurationFormatter {
    val threshold: Duration
}
