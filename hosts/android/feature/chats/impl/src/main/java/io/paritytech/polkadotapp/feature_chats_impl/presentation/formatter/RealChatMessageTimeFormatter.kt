package io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.R
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.common.presentation.formatters.time.RelativeDateUtils.getWeekdayName
import io.paritytech.polkadotapp.common.presentation.formatters.time.TimeFormatter
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.time.Duration.Companion.days
import kotlin.time.Duration.Companion.hours
import kotlin.time.Duration.Companion.minutes

private val ONE_MINUTE_MILLIS = 1.minutes.inWholeMilliseconds
private val ONE_HOUR_MILLIS = 1.hours.inWholeMilliseconds
private val ONE_DAY_MILLIS = 1.days.inWholeMilliseconds
private val TWO_DAYS_MILLIS = 2.days.inWholeMilliseconds
private val ONE_WEEK_MILLIS = 7.days.inWholeMilliseconds
private val ONE_YEAR_MILLIS = 365.days.inWholeMilliseconds
private const val DATE_PATTERN = "dd.MM"
private const val DATE_WITH_YEAR_PATTERN = "dd.MM.yyyy"

@Singleton
class RealChatMessageTimeFormatter @Inject constructor(
    @param:ApplicationContext private val context: Context,
    private val timeFormatter: TimeFormatter,
) : ChatMessageTimeFormatter {
    override fun formatMessageTime(time: Timestamp): String {
        return timeFormatter.formatTime24h(time)
    }

    override fun formatChatListTime(time: Timestamp, relativeTo: Timestamp): String {
        if (time > relativeTo) return context.getString(R.string.common_relative_time_now)

        val diff = relativeTo - time

        return when {
            diff < ONE_MINUTE_MILLIS -> context.getString(R.string.common_relative_time_now)

            diff < ONE_HOUR_MILLIS -> {
                val minutes = (diff / ONE_MINUTE_MILLIS).toInt()
                context.resources.getQuantityString(
                    R.plurals.common_relative_time_minutes_ago,
                    minutes,
                    minutes
                )
            }

            diff < ONE_DAY_MILLIS -> {
                val hours = (diff / ONE_HOUR_MILLIS).toInt()
                context.resources.getQuantityString(
                    R.plurals.common_relative_time_hours_ago,
                    hours,
                    hours
                )
            }

            diff < TWO_DAYS_MILLIS -> context.getString(R.string.common_relative_time_yesterday)

            diff < ONE_WEEK_MILLIS -> getWeekdayName(time)

            diff < ONE_YEAR_MILLIS -> SimpleDateFormat(DATE_PATTERN, Locale.getDefault()).format(Date(time))

            else -> SimpleDateFormat(DATE_WITH_YEAR_PATTERN, Locale.getDefault()).format(Date(time))
        }
    }
}
