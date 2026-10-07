package io.paritytech.polkadotapp.feature_videogame_impl.data.calendar

import android.content.Context
import io.paritytech.polkadotapp.common.BuildConfig
import io.paritytech.polkadotapp.common.R as RCommon

internal fun Context.gameCalendarEventTitle(): String =
    getString(RCommon.string.video_game_calendar_event_title, BuildConfig.APP_NAME)
