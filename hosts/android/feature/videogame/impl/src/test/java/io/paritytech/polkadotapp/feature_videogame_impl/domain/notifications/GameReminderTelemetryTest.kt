package io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications

import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.sentry.SentryLevel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class GameReminderTelemetryTest {
    private val game = ProductId.fromStoredValue("game.dot")
    @Test
    fun `scheduled carries the product, the start and a negative fire delay as numbers`() {
        val event = GameReminderEvent.Scheduled(game, START, GameReminderDelivery.ALARM).toSentryEvent(NOW)
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
        val event = GameReminderEvent.NotScheduled(game, START, GameReminderSkipReason.START_IN_PAST).toSentryEvent(NOW)
        assertEquals("host.reminder.not_scheduled", event.message?.formatted)
        assertEquals("start_in_past", event.getTag("reminder.skip_reason"))
    }

    @Test
    fun `fired carries the lateness, the delivery and whether the app was in the foreground`() {
        val event = GameReminderEvent.Fired(
            game,
            NOW - 1_500,
            foreground = false,
            stillScheduled = true,
            delivery = GameReminderDelivery.NOTIFICATION,
        ).toSentryEvent(NOW)
        assertEquals("host.reminder.fired", event.message?.formatted)
        assertEquals(1_500L, event.getExtra("reminder.delay_after_start_ms"))
        assertEquals("background", event.getTag("host.app_state"))
        assertEquals("true", event.getTag("reminder.still_scheduled"))
        assertEquals("notification", event.getTag("reminder.delivery"))
    }

    @Test
    fun `product opened does not invent an open source`() {
        val event = GameReminderEvent.ProductOpened(game, START).toSentryEvent(NOW)
        assertEquals("host.reminder.product_opened", event.message?.formatted)
        assertNull(event.getTag("reminder.open_source"))
    }

    @Test
    fun `an event without a known start carries no start and no fire delay`() {
        val event = GameReminderEvent.Tapped(game, startsAtMillis = null).toSentryEvent(NOW)
        assertEquals("host.reminder.tapped", event.message?.formatted)
        assertNull(event.getExtra("reminder.starts_at_unix_ms"))
        assertNull(event.getExtra("reminder.delay_after_start_ms"))
    }

    @Test
    fun `cancelled is named for what happened`() {
        val event = GameReminderEvent.Cancelled(game, START).toSentryEvent(NOW)
        assertEquals("host.reminder.cancelled", event.message?.formatted)
    }

    private companion object {
        const val NOW = 1_000_000_000_000L
        const val START = NOW + 60_000
    }
}
