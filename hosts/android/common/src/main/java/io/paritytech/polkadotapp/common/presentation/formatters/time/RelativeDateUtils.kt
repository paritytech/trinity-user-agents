package io.paritytech.polkadotapp.common.presentation.formatters.time

import io.paritytech.polkadotapp.common.domain.model.Timestamp
import java.util.Calendar
import java.util.TimeZone

/**
 * Utility for date comparisons using timestamp arithmetic.
 * Avoids Calendar allocations for day-based comparisons.
 */
object RelativeDateUtils {
    private const val MILLIS_PER_DAY = 24 * 60 * 60 * 1000L

    /**
     * Gets the day number since epoch, adjusted for timezone.
     */
    private fun getDayOfTimestamp(timestamp: Timestamp): Long {
        val offset = TimeZone.getDefault().getOffset(timestamp).toLong()
        return (timestamp + offset) / MILLIS_PER_DAY
    }

    /**
     * Checks if two timestamps fall on the same calendar day.
     * No Calendar allocation.
     */
    fun isSameDay(timestamp1: Timestamp, timestamp2: Timestamp): Boolean {
        return getDayOfTimestamp(timestamp1) == getDayOfTimestamp(timestamp2)
    }

    /**
     * Checks if [timestamp] is yesterday relative to [relativeTo].
     * No Calendar allocation.
     */
    fun isYesterday(timestamp: Timestamp, relativeTo: Timestamp): Boolean {
        return getDayOfTimestamp(relativeTo) - getDayOfTimestamp(timestamp) == 1L
    }

    /**
     * Gets the number of days between two timestamps.
     * Positive if [timestamp] is before [relativeTo].
     * No Calendar allocation.
     */
    fun getDaysDifference(timestamp: Timestamp, relativeTo: Timestamp): Long {
        return getDayOfTimestamp(relativeTo) - getDayOfTimestamp(timestamp)
    }

    /**
     * Checks if [timestamp] is within the last [days] days relative to [relativeTo].
     * Excludes same day and yesterday.
     * No Calendar allocation.
     */
    fun isWithinDays(timestamp: Timestamp, relativeTo: Timestamp, days: Int): Boolean {
        val diff = getDaysDifference(timestamp, relativeTo)
        return diff in 2 until days
    }

    /**
     * Checks if two timestamps fall in the same calendar year.
     * Uses single Calendar instance (reused).
     */
    fun isSameYear(timestamp1: Timestamp, timestamp2: Timestamp): Boolean {
        val cal = Calendar.getInstance()
        cal.timeInMillis = timestamp1
        val year1 = cal.get(Calendar.YEAR)
        cal.timeInMillis = timestamp2
        return year1 == cal.get(Calendar.YEAR)
    }

    /**
     * Gets the weekday name for a timestamp.
     * Uses Calendar for locale-aware weekday name.
     */
    fun getWeekdayName(timestamp: Timestamp): String {
        val cal = Calendar.getInstance()
        cal.timeInMillis = timestamp
        return cal.getDisplayName(Calendar.DAY_OF_WEEK, Calendar.LONG, java.util.Locale.getDefault()).orEmpty()
    }

    /**
     * Returns true if [timestamp] falls within the last week relative to [relativeTo],
     * excluding today and yesterday.
     */
    fun isWithinLastWeekExcludingYesterday(timestamp: Timestamp, relativeTo: Timestamp): Boolean {
        val diff = getDaysDifference(timestamp, relativeTo)
        return diff in 2..6
    }
}
