package io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.data.telemetry.HostEvent
import io.paritytech.polkadotapp.common.data.telemetry.HostEventCapture
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOrigin
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOriginProvider
import io.paritytech.polkadotapp.common.data.time.TimeProvider
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
    private val originProvider = HostEventOriginProvider {
        HostEventOrigin(
            appVariant = "debug",
            appFlavor = "nightly",
            nativeAppVersion = "1.2.3",
            platform = "android",
            osVersion = "35",
            appBuild = 42,
        )
    }

    @Test
    fun `scheduled carries the product, the start and a negative fire delay as numbers`() {
        val event = reported(GameReminderEvent.Scheduled(game, START, GameReminderDelivery.ALARM))
        assertEquals("host.reminder.scheduled", event.name)
        assertEquals("android", event.tags["host.platform"])
        assertEquals("alarm", event.tags["reminder.delivery"])
        assertEquals("game.dot", event.tags["reminder.product_id"])
        assertEquals(START, event.extras["reminder.starts_at_unix_ms"])
        assertEquals(NOW - START, event.extras["reminder.delay_after_start_ms"])
    }

    @Test
    fun `not scheduled carries the skip reason`() {
        val event = reported(GameReminderEvent.NotScheduled(game, START, GameReminderSkipReason.START_IN_PAST))
        assertEquals("host.reminder.not_scheduled", event.name)
        assertEquals("start_in_past", event.tags["reminder.skip_reason"])
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
        assertEquals("host.reminder.fired", event.name)
        assertEquals(1_500L, event.extras["reminder.delay_after_start_ms"])
        assertEquals("background", event.tags["host.app_state"])
        assertEquals("true", event.tags["reminder.still_scheduled"])
        assertEquals("notification", event.tags["reminder.delivery"])
    }

    @Test
    fun `an event without a known start carries no start and no fire delay`() {
        val event = reported(GameReminderEvent.Tapped(game, startsAtMillis = null))
        assertEquals("host.reminder.tapped", event.name)
        assertNull(event.extras["reminder.starts_at_unix_ms"])
        assertNull(event.extras["reminder.delay_after_start_ms"])
    }

    @Test
    fun `a reminder event carries the base tags every other host event carries`() {
        val event = reported(GameReminderEvent.Tapped(game, START))
        assertEquals("android", event.tags["host.platform"])
        assertEquals("debug", event.tags["host.app_variant"])
        assertEquals("nightly", event.tags["host.app_flavor"])
        assertEquals("1.2.3", event.tags["host.native_app_version"])
        assertEquals(42L, event.extras["host.app_build"])
    }

    @Test
    fun `the event is not built when capture drops it`() {
        GameReminderTelemetry(timeProvider, originProvider, HostEventCapture { })
            .report(GameReminderEvent.Tapped(game, START))

        verify(exactly = 0) { timeProvider.now() }
    }

    private fun reported(event: GameReminderEvent): HostEvent {
        var captured: HostEvent? = null
        GameReminderTelemetry(timeProvider, originProvider, HostEventCapture { captured = it() }).report(event)
        return checkNotNull(captured) { "no event was captured" }
    }

    private companion object {
        const val NOW = 1_000_000_000_000L
        const val START = NOW + 60_000
    }
}
