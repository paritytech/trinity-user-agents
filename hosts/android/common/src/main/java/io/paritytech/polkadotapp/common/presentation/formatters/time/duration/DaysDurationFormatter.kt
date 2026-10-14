package io.paritytech.polkadotapp.common.presentation.formatters.time.duration

import android.content.Context
import io.paritytech.polkadotapp.common.R
import io.paritytech.polkadotapp.common.utils.lastDays
import kotlin.time.Duration
import kotlin.time.Duration.Companion.days

internal class DaysDurationFormatter(
    private val context: Context,
) : BoundedDurationFormatter {
    override val threshold: Duration = 1.days

    override fun format(duration: Duration): String {
        val days = duration.lastDays

        return context.resources.getQuantityString(R.plurals.common_days_format, days.toInt(), days)
    }
}
