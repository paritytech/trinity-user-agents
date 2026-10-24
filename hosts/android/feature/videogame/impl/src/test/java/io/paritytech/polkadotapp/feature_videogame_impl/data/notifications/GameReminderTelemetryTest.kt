package io.paritytech.polkadotapp.feature_videogame_impl.data.notifications

import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.sentry.SentryEvent
import io.sentry.SentryLevel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class GameReminderTelemetryTest {
    private val game = ProductId.fromStoredValue("game.dot")
    private val captured = mutableListOf<SentryEvent>()
    private var enabled = true

    private val telemetry = GameReminderTelemetry.createForTest(
        enabled = { enabled },
        nowMillis = { NOW },
        capture = { captured += it },
    )

    @Test
    fun `scheduled carries the product, the start and a negative fire delay as numbers`() {
        telemetry.report(GameReminderEvent.Scheduled(game, START, exact = true))

        val event = singleEvent()
        assertEquals("host.reminder.scheduled", event.message?.formatted)
        assertEquals(SentryLevel.INFO, event.level)
        assertEquals("host.telemetry", event.logger)
        assertEquals("android", event.getTag("host.platform"))
        assertEquals("true", event.getTag("reminder.exact"))
        assertEquals("game.dot", event.getTag("reminder.product_id"))
        assertEquals(START, event.getExtra("reminder.starts_at_unix_ms"))
        assertEquals(NOW - START, event.getExtra("reminder.delay_after_start_ms"))
    }

    @Test
    fun `not scheduled carries the skip reason`() {
        telemetry.report(GameReminderEvent.NotScheduled(game, START))

        val event = singleEvent()
        assertEquals("host.reminder.not_scheduled", event.message?.formatted)
        assertEquals("not_authorized", event.getTag("reminder.skip_reason"))
    }

    @Test
    fun `exact alarm denied is its own event`() {
        telemetry.report(GameReminderEvent.ExactAlarmDenied(game, START))

        assertEquals("host.reminder.exact_alarm_denied", singleEvent().message?.formatted)
    }

    @Test
    fun `fired carries the lateness and whether the app was in the foreground`() {
        telemetry.report(GameReminderEvent.Fired(game, NOW - 1_500, foreground = false, stillScheduled = true))

        val event = singleEvent()
        assertEquals("host.reminder.fired", event.message?.formatted)
        assertEquals(1_500L, event.getExtra("reminder.delay_after_start_ms"))
        assertEquals("background", event.getTag("host.app_state"))
        assertEquals("true", event.getTag("reminder.still_scheduled"))
    }

    @Test
    fun `product opened carries its source`() {
        telemetry.report(GameReminderEvent.ProductOpened(game, START, GameReminderOpenSource.ALARM))

        val event = singleEvent()
        assertEquals("host.reminder.product_opened", event.message?.formatted)
        assertEquals("alarm", event.getTag("reminder.open_source"))
    }

    @Test
    fun `an event without a known start carries no start and no fire delay`() {
        telemetry.report(GameReminderEvent.Tapped(game, startsAtMillis = null))

        val event = singleEvent()
        assertEquals("host.reminder.tapped", event.message?.formatted)
        assertNull(event.getExtra("reminder.starts_at_unix_ms"))
        assertNull(event.getExtra("reminder.delay_after_start_ms"))
    }

    @Test
    fun `dismissed and cancelled are named for what happened`() {
        telemetry.report(GameReminderEvent.Dismissed(game, START))
        telemetry.report(GameReminderEvent.Cancelled(game, START))

        assertEquals(listOf("host.reminder.dismissed", "host.reminder.cancelled"), captured.map { it.message?.formatted })
    }

    @Test
    fun `captures nothing when host telemetry is off`() {
        enabled = false

        telemetry.report(GameReminderEvent.Scheduled(game, START, exact = true))

        assertTrue(captured.isEmpty())
    }

    private fun singleEvent(): SentryEvent {
        assertEquals(1, captured.size)
        return captured.single()
    }

    private companion object {
        const val NOW = 1_000_000_000_000L
        const val START = NOW + 60_000
    }
}
