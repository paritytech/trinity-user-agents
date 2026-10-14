package io.paritytech.polkadotapp.common.presentation.formatters.time.duration

import kotlin.time.Duration

internal class ZeroDurationFormatter(
    private val nestedFormatter: DurationFormatter,
) : BoundedDurationFormatter {
    override val threshold: Duration = Duration.ZERO

    override fun format(duration: Duration): String {
        return nestedFormatter.format(duration)
    }
}
