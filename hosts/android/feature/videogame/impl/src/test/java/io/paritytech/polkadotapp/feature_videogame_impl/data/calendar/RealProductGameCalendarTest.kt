package io.paritytech.polkadotapp.feature_videogame_impl.data.calendar

import android.content.Context
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.slot
import io.paritytech.polkadotapp.common.utils.calendar.CalendarEvent
import io.paritytech.polkadotapp.common.utils.calendar.CalendarEventsMixin
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import kotlin.time.Duration.Companion.minutes

class RealProductGameCalendarTest {
    private val mixin: CalendarEventsMixin = mockk {
        coEvery { addEventIfPermitted(any(), any()) } returns Result.success(Unit)
    }
    private val context: Context = mockk {
        every { getString(any(), *anyVararg()) } returns TITLE
    }
    private val calendar = RealProductGameCalendar(mixin, context)

    @Test
    fun `adds a titled half-hour event with an alert five minutes before`() = runTest {
        calendar.addGame(START)

        val event = slot<CalendarEvent>()
        coVerify(exactly = 1) { mixin.addEventIfPermitted(capture(event), 5.minutes) }
        assertEquals(START, event.captured.timeStart)
        assertEquals(30.minutes, event.captured.duration)
        assertEquals(TITLE, event.captured.title)
    }

    private companion object {
        const val START = 1_000_000_000_000L
        const val TITLE = "Game"
    }
}
