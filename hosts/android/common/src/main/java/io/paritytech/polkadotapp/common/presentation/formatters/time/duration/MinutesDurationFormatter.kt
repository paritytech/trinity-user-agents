package io.paritytech.polkadotapp.common.presentation.formatters.time.duration

import android.content.Context
import io.paritytech.polkadotapp.common.R
import io.paritytech.polkadotapp.common.utils.lastMinutes
import kotlin.time.Duration
import kotlin.time.Duration.Companion.minutes

internal class MinutesDurationFormatter(
    private val context: Context,
) : BoundedDurationFormatter {
    override val threshold: Duration = 1.minutes

    override fun format(duration: Duration): String {
        val minutes = duration.lastMinutes

        return context.resources.getQuantityString(R.plurals.common_minutes_format, minutes, minutes)
    }
}
