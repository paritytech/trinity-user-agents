package io.paritytech.polkadotapp.feature_videogame_impl.data.calendar

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.utils.calendar.CalendarEvent
import io.paritytech.polkadotapp.common.utils.calendar.CalendarEventsMixin
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.ProductGameCalendar
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.time.Duration.Companion.minutes

@Singleton
class RealProductGameCalendar @Inject constructor(
    private val calendarEventsMixin: CalendarEventsMixin,
    @ApplicationContext private val context: Context,
) : ProductGameCalendar {
    override suspend fun addGame(startsAtMillis: Long) {
        val event = CalendarEvent(
            timeStart = startsAtMillis,
            duration = EVENT_DURATION,
            title = context.gameCalendarEventTitle(),
        )
        calendarEventsMixin.addEventIfPermitted(event, ALERT_BEFORE)
            .logFailure("product game calendar event")
    }

    private companion object {
        val EVENT_DURATION = 30.minutes
        val ALERT_BEFORE = 5.minutes
    }
}
