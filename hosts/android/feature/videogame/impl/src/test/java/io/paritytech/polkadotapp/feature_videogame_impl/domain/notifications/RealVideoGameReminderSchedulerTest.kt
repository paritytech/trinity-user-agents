package io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.test_shared.FakeTimeProvider
import org.junit.Assert.assertThrows
import org.junit.Test

class RealVideoGameReminderSchedulerTest {
    private val alarms: GameReminderAlarms = mockk(relaxUnitFun = true)
    private val telemetry: GameReminderTelemetry = mockk(relaxUnitFun = true)

    private val scheduler = RealVideoGameReminderScheduler(alarms, FakeTimeProvider { NOW }, telemetry)

    private val game = ProductId.fromStoredValue("game.dot")

    @Test
    fun `an exact alarm is reported as scheduled for alarm delivery`() {
        withExactAlarms(allowed = true)

        scheduler.scheduleProductGameStart(game, START)

        verify(exactly = 1) { alarms.set(game, START, exact = true) }
        verifyReported(GameReminderEvent.Scheduled(game, START, GameReminderDelivery.ALARM))
    }

    @Test
    fun `without exact alarms the reminder is reported as scheduled for notification delivery`() {
        withExactAlarms(allowed = false)

        scheduler.scheduleProductGameStart(game, START)

        verify(exactly = 1) { alarms.set(game, START, exact = false) }
        verifyReported(GameReminderEvent.Scheduled(game, START, GameReminderDelivery.NOTIFICATION))
    }

    @Test
    fun `a start already past cancels the alarm and is reported as not scheduled`() {
        scheduler.scheduleProductGameStart(game, NOW)

        verify(exactly = 1) { alarms.cancel(game) }
        verify(exactly = 0) { alarms.set(any(), any(), any()) }
        verifyReported(GameReminderEvent.NotScheduled(game, NOW, GameReminderSkipReason.START_IN_PAST))
    }

    @Test
    fun `an alarm the OS refuses is reported as failed and still thrown`() {
        withExactAlarms(allowed = true)
        every { alarms.set(game, START, exact = true) } throws SecurityException("revoked")

        assertThrows(SecurityException::class.java) { scheduler.scheduleProductGameStart(game, START) }

        verifyReported(GameReminderEvent.NotScheduled(game, START, GameReminderSkipReason.FAILED))
        verifyNotReported(GameReminderEvent.Scheduled(game, START, GameReminderDelivery.ALARM))
    }

    private fun withExactAlarms(allowed: Boolean) {
        every { alarms.canRingExactly() } returns allowed
    }

    private fun verifyReported(event: GameReminderEvent) {
        verify(exactly = 1) { telemetry.report(event) }
    }

    private fun verifyNotReported(event: GameReminderEvent) {
        verify(exactly = 0) { telemetry.report(event) }
    }

    private companion object {
        const val NOW = 1_000_000_000_000L
        const val START = NOW + 60_000
    }
}
