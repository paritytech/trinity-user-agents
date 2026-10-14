package io.paritytech.polkadotapp.common.presentation.formatters.time

import android.content.Context
import android.text.format.DateUtils
import androidx.compose.runtime.staticCompositionLocalOf
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.R
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.common.presentation.formatters.time.TimeFormatter.BigPeriodMode
import io.paritytech.polkadotapp.common.presentation.formatters.time.duration.CompoundDurationFormatter
import io.paritytech.polkadotapp.common.presentation.formatters.time.duration.DayAndHourDurationFormatter
import io.paritytech.polkadotapp.common.presentation.formatters.time.duration.DaysDurationFormatter
import io.paritytech.polkadotapp.common.presentation.formatters.time.duration.DescriptiveBigPeriodsFormatter
import io.paritytech.polkadotapp.common.presentation.formatters.time.duration.DurationFormatter
import io.paritytech.polkadotapp.common.presentation.formatters.time.duration.HoursDurationFormatter
import io.paritytech.polkadotapp.common.presentation.formatters.time.duration.MinutesDurationFormatter
import io.paritytech.polkadotapp.common.presentation.formatters.time.duration.RoundMinutesDurationFormatter
import io.paritytech.polkadotapp.common.presentation.formatters.time.duration.ShortBigPeriodsFormatter
import io.paritytech.polkadotapp.common.presentation.formatters.time.duration.ZeroDurationFormatter
import io.paritytech.polkadotapp.design.utils.noLocalProvidedFor
import java.text.DateFormat
import java.text.SimpleDateFormat
import java.util.Calendar
import java.util.Date
import java.util.Locale
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.time.Duration
import android.text.format.DateFormat as AndroidDateFormat

val LocalTimeFormatter = staticCompositionLocalOf<TimeFormatter> {
    noLocalProvidedFor("TimeFormatter")
}

interface TimeFormatter {
    companion object {
        fun mocked(context: Context) = RealTimeFormatter(context)
    }

    fun formatDateTime(timestamp: Timestamp): String

    fun formatShortDateTime(timestamp: Timestamp): String

    fun formatDate(timestamp: Timestamp): String

    fun formatTime(timestamp: Timestamp): String

    fun formatDuration(duration: Duration): String

    /**
     * Formats duration in a form MM:SS
     */
    fun formatTimer(duration: Duration): String

    /**
     * Compact countdown:
     *   < 1 minute → bare seconds ("59")
     *   < 1 hour   → M:SS without a leading zero on minutes ("1:59")
     *   ≥ 1 hour   → H:MM:SS ("2:00:30")
     */
    fun formatCountdown(duration: Duration): String

    fun formatTimeLeft(duration: Duration): String

    /**
     * Intended for formatting long periods, e.g. weeks or months
     *
     * Formats the longest integral period starting from days:
     * 24 hours -> 24H
     * 25 hours -> 1D
     * 3 days -> 3D
     * 7 days -> 1W
     * 15 days -> 15D
     * 14 days -> 1W
     */
    fun formatBigPeriod(duration: Duration, mode: BigPeriodMode): String

    fun formatDayOfMonth(timestamp: Timestamp): Int

    fun formatMonth(timestamp: Timestamp, isShort: Boolean): String

    fun formatMonthNumber(timestamp: Timestamp): Int

    fun formatTime24h(timestamp: Timestamp): String

    /**
     * Format used for the past-game card header: "Jan 14 at 4:00 PM".
     */
    fun formatGameDateTime(timestamp: Timestamp): String

    /** Absolute date with weekday, e.g. "Thu, 14 Jan" (adds the year when [includeYear]). */
    fun formatDateWithWeekday(timestamp: Timestamp, includeYear: Boolean): String

    /**
     * Formats date for chat date separators.
     * Returns: "Today", "Yesterday", "Wed, 27 Nov", or "Wed, 27 Nov 2023" for different year.
     * Pass [useRelativeLabels] = false to always use the absolute date (no "Today"/"Yesterday").
     */
    fun formatChatDateSeparator(
        timestamp: Timestamp,
        relativeTo: Timestamp,
        useRelativeLabels: Boolean = true,
    ): String

    enum class BigPeriodMode {
        /**
         * Format in a form of 24H / 3D / 1W
         */
        SHORT,

        /**
         * Format in a form of 20 hours / 3 days / 2 weeks
         */
        DESCRIPTIVE
    }
}

fun TimeFormatter.formatBigPeriodShort(duration: Duration): String {
    return formatBigPeriod(duration, BigPeriodMode.SHORT)
}

fun TimeFormatter.formatBigPeriodDescriptive(duration: Duration): String {
    return formatBigPeriod(duration, BigPeriodMode.DESCRIPTIVE)
}

@Singleton
class RealTimeFormatter @Inject constructor(
    @param:ApplicationContext private val context: Context
) : TimeFormatter {
    private val dateTimeFormat = SimpleDateFormat.getDateTimeInstance()

    private val durationFormatter = createDurationFormatter()

    private val shortBigPeriodFormatter = ShortBigPeriodsFormatter(context)

    private val descriptiveBigPeriodFormatter = DescriptiveBigPeriodsFormatter(context)

    private val gameDateTimeFormat: SimpleDateFormat by lazy {
        val locale = Locale.getDefault()
        val pattern = AndroidDateFormat.getBestDateTimePattern(locale, "MMMd jmm")
        SimpleDateFormat(pattern, locale)
    }

    override fun formatDateTime(timestamp: Timestamp): String {
        return dateTimeFormat.format(Date(timestamp))
    }

    override fun formatShortDateTime(timestamp: Timestamp): String {
        val dateFormat = DateFormat.getDateInstance(DateFormat.SHORT, Locale.getDefault())
        val timeFormat = DateFormat.getTimeInstance(DateFormat.SHORT, Locale.getDefault())
        val date = Date(timestamp)
        return "${dateFormat.format(date)}, ${timeFormat.format(date)}"
    }

    override fun formatDate(timestamp: Timestamp): String {
        return DateUtils.formatDateTime(
            context,
            timestamp,
            DateUtils.FORMAT_SHOW_DATE or DateUtils.FORMAT_SHOW_YEAR or DateUtils.FORMAT_ABBREV_MONTH,
        )
    }

    override fun formatTime(timestamp: Timestamp): String {
        return DateUtils.formatDateTime(
            context,
            timestamp,
            DateUtils.FORMAT_SHOW_TIME,
        )
    }

    override fun formatDuration(duration: Duration): String {
        return durationFormatter.format(duration)
    }

    override fun formatTimer(duration: Duration): String {
        return duration.toComponents { minutes, seconds, _ ->
            String.format("%02d:%02d", minutes, seconds)
        }
    }

    override fun formatCountdown(duration: Duration): String {
        return duration.toComponents { hours, minutes, seconds, _ ->
            when {
                hours > 0 -> String.format(Locale.ROOT, "%d:%02d:%02d", hours, minutes, seconds)
                minutes > 0 -> String.format(Locale.ROOT, "%d:%02d", minutes, seconds)
                else -> seconds.toString()
            }
        }
    }

    override fun formatTimeLeft(duration: Duration): String {
        return duration.toComponents { days, hours, minutes, seconds, _ ->
            buildString {
                val daysVisible = days > 0
                val hoursVisible = hours > 0 || daysVisible
                val minutesVisible = minutes > 0 || hoursVisible

                if (daysVisible) append(String.format("%02d", days) + "d ")
                if (hoursVisible) append(String.format("%02d", hours) + "h ")
                if (minutesVisible) append(String.format("%02d", minutes) + "m ")

                if (daysVisible.not()) append(String.format("%02d", seconds) + "s ")
            }
        }
    }

    override fun formatBigPeriod(duration: Duration, mode: BigPeriodMode): String {
        val formatter = when (mode) {
            BigPeriodMode.SHORT -> shortBigPeriodFormatter
            BigPeriodMode.DESCRIPTIVE -> descriptiveBigPeriodFormatter
        }

        return formatter.format(duration)
    }

    override fun formatDayOfMonth(timestamp: Timestamp): Int {
        val calendar = Calendar.getInstance()
        calendar.timeInMillis = timestamp

        return calendar.get(Calendar.DAY_OF_MONTH)
    }

    override fun formatMonth(timestamp: Timestamp, isShort: Boolean): String {
        val calendar = Calendar.getInstance()
        calendar.timeInMillis = timestamp

        val style = if (isShort) Calendar.SHORT else Calendar.LONG

        return calendar.getDisplayName(Calendar.MONTH, style, Locale.getDefault()).orEmpty()
    }

    override fun formatMonthNumber(timestamp: Timestamp): Int {
        val calendar = Calendar.getInstance()
        calendar.timeInMillis = timestamp

        return calendar.get(Calendar.MONTH) + 1
    }

    override fun formatTime24h(timestamp: Timestamp): String {
        return SimpleDateFormat("H:mm", Locale.getDefault()).format(Date(timestamp))
    }

    override fun formatGameDateTime(timestamp: Timestamp): String {
        return gameDateTimeFormat.format(Date(timestamp))
    }

    override fun formatChatDateSeparator(timestamp: Timestamp, relativeTo: Timestamp, useRelativeLabels: Boolean): String {
        return when {
            useRelativeLabels && RelativeDateUtils.isSameDay(timestamp, relativeTo) -> context.getString(R.string.common_relative_time_today)
            useRelativeLabels && RelativeDateUtils.isYesterday(timestamp, relativeTo) -> context.getString(R.string.common_relative_time_yesterday)
            RelativeDateUtils.isSameYear(timestamp, relativeTo) -> formatDateWithWeekday(timestamp, includeYear = false)
            else -> formatDateWithWeekday(timestamp, includeYear = true)
        }
    }

    override fun formatDateWithWeekday(timestamp: Timestamp, includeYear: Boolean): String {
        val pattern = if (includeYear) "EEE, d MMM yyyy" else "EEE, d MMM"
        return SimpleDateFormat(pattern, Locale.getDefault()).format(Date(timestamp))
    }

    private fun createDurationFormatter(): DurationFormatter {
        val hoursFormatter = HoursDurationFormatter(context)
        val daysFormatter = DaysDurationFormatter(context)

        val daysAndHoursFormatter = DayAndHourDurationFormatter(daysFormatter, hoursFormatter)

        val compoundFormatter = CompoundDurationFormatter(
            daysAndHoursFormatter,
            hoursFormatter,
            MinutesDurationFormatter(context),
            ZeroDurationFormatter(daysFormatter),
        )

        return RoundMinutesDurationFormatter(compoundFormatter)
    }
}
