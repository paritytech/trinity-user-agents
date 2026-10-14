package io.paritytech.polkadotapp.common.presentation.formatters.time.duration

import android.content.Context
import io.paritytech.polkadotapp.common.R
import kotlin.time.Duration

abstract class BigPeriodsFormatter(
    private val fullDayHoursInclusive: Boolean // True if 24 hours should be still displayed as hours
) : DurationFormatter {
    protected abstract fun formatHours(hours: Int): String

    protected abstract fun formatDays(days: Int): String

    protected abstract fun formatWeeks(weeks: Int): String

    override fun format(duration: Duration): String {
        return when {
            duration.smallEnoughForHours() -> formatHours(duration.inWholeHours.toInt())
            duration.inWholeDays.toInt() % 7 == 0 -> formatWeeks(duration.inWholeDays.toInt() / 7)
            else -> formatDays(duration.inWholeDays.toInt())
        }
    }

    private fun Duration.smallEnoughForHours(): Boolean {
        return if (fullDayHoursInclusive) {
            inWholeHours <= 24
        } else {
            inWholeHours < 24
        }
    }
}

class ShortBigPeriodsFormatter(private val context: Context) : BigPeriodsFormatter(fullDayHoursInclusive = true) {
    override fun formatHours(hours: Int): String {
        return context.getString(R.string.common_duration_format_long_period_hours, hours)
    }

    override fun formatDays(days: Int): String {
        return context.getString(R.string.common_duration_format_long_period_days, days)
    }

    override fun formatWeeks(weeks: Int): String {
        return context.getString(R.string.common_duration_format_long_period_weeks, weeks)
    }
}

class DescriptiveBigPeriodsFormatter(private val context: Context) : BigPeriodsFormatter(fullDayHoursInclusive = false) {
    override fun formatHours(hours: Int): String {
        return context.resources.getQuantityString(R.plurals.common_hours_format, hours, hours)
    }

    override fun formatDays(days: Int): String {
        return context.resources.getQuantityString(R.plurals.common_days_format, days, days)
    }

    override fun formatWeeks(weeks: Int): String {
        return context.resources.getQuantityString(R.plurals.common_weeks_format, weeks, weeks)
    }
}
