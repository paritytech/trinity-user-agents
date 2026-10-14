package io.paritytech.polkadotapp.common.presentation.formatters.time.duration

import kotlin.time.Duration

internal class CompoundDurationFormatter(
    private val formatters: List<BoundedDurationFormatter>,
) : DurationFormatter {
    constructor(vararg formatters: BoundedDurationFormatter) : this(formatters.toList())

    override fun format(duration: Duration): String {
        val formatter = formatters.firstOrNull { it.threshold <= duration } ?: formatters.last()

        return formatter.format(duration)
    }
}
