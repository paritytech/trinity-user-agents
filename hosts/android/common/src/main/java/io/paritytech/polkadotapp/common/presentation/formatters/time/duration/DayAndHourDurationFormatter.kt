
package io.paritytech.polkadotapp.common.presentation.formatters.time.duration

import io.paritytech.polkadotapp.common.utils.lastHours
import kotlin.time.Duration
import kotlin.time.Duration.Companion.days

internal class DayAndHourDurationFormatter(
    private val dayFormatter: DaysDurationFormatter,
    private val hoursFormatter: HoursDurationFormatter,
    private val format: String? = null,
) : BoundedDurationFormatter {
    override val threshold: Duration = 1.days

    override fun format(duration: Duration): String {
        return if (duration.lastHours > 0) {
            formatDaysAndHours(duration)
        } else {
            dayFormatter.format(duration)
        }
    }

    @Suppress("IfThenToElvis")
    private fun formatDaysAndHours(duration: Duration): String {
        return if (format == null) {
            dayFormatter.format(duration) + " " + hoursFormatter.format(duration)
        } else {
            format.format(
                dayFormatter.format(duration),
                hoursFormatter.format(duration),
            )
        }
    }
}
