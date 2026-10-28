package io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.sentry.SentryEvent
import io.sentry.SentryLevel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import uniffi.truapi.ProductId
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

@OptIn(ExperimentalTime::class)
class GameReminderTelemetryTest {
    private val game = ProductId.parse("game.dot").getOrThrow()
    private val timeProvider = mockk<TimeProvider> {
        every { now() } returns Instant.fromEpochMilliseconds(NOW)
    }

    @Test
    fun `scheduled carries the product, the start and a negative fire delay as numbers`() {
        val event = reported(GameReminderEvent.Scheduled(game, START, GameReminderDelivery.ALARM))
        assertEquals("host.reminder.scheduled", event.message?.formatted)
        assertEquals(SentryLevel.INFO, event.level)
        assertEquals("host.telemetry", event.logger)
        assertEquals("android", event.getTag("host.platform"))
        assertEquals("alarm", event.getTag("reminder.delivery"))
        assertEquals("game.dot", event.getTag("reminder.product_id"))
        assertEquals(START, event.getExtra("reminder.starts_at_unix_ms"))
        assertEquals(NOW - START, event.getExtra("reminder.delay_after_start_ms"))
    }

    @Test
    fun `not scheduled carries the skip reason`() {
        val event = reported(GameReminderEvent.NotScheduled(game, START, GameReminderSkipReason.START_IN_PAST))
        assertEquals("host.reminder.not_scheduled", event.message?.formatted)
        assertEquals("start_in_past", event.getTag("reminder.skip_reason"))
    }

    @Test
    fun `fired carries the lateness, the delivery and whether the app was in the foreground`() {
        val event = reported(
            GameReminderEvent.Fired(
                game,
                NOW - 1_500,
                foreground = false,
                stillScheduled = true,
                delivery = GameReminderDelivery.NOTIFICATION,
            ),
        )
        assertEquals("host.reminder.fired", event.message?.formatted)
        assertEquals(1_500L, event.getExtra("reminder.delay_after_start_ms"))
        assertEquals("background", event.getTag("host.app_state"))
        assertEquals("true", event.getTag("reminder.still_scheduled"))
        assertEquals("notification", event.getTag("reminder.delivery"))
    }

    @Test
    fun `an event without a known start carries no start and no fire delay`() {
        val event = reported(GameReminderEvent.Tapped(game, startsAtMillis = null))
        assertEquals("host.reminder.tapped", event.message?.formatted)
        assertNull(event.getExtra("reminder.starts_at_unix_ms"))
        assertNull(event.getExtra("reminder.delay_after_start_ms"))
    }

    @Test
    fun `the event is not built when capture drops it`() {
        GameReminderTelemetry(timeProvider, GameReminderEventCapture { }).report(GameReminderEvent.Tapped(game, START))

        verify(exactly = 0) { timeProvider.now() }
    }

    private fun reported(event: GameReminderEvent): SentryEvent {
        var captured: SentryEvent? = null
        GameReminderTelemetry(timeProvider, GameReminderEventCapture { captured = it() }).report(event)
        return checkNotNull(captured) { "no event was captured" }
    }

    private companion object {
        const val NOW = 1_000_000_000_000L
        const val START = NOW + 60_000
    }
}
