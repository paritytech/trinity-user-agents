package io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications

import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.test_shared.FakeTimeProvider
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.whenever
import org.junit.Assert.assertThrows
import org.junit.Test
import org.mockito.Mockito.anyBoolean
import org.mockito.Mockito.anyLong
import org.mockito.Mockito.doThrow
import org.mockito.Mockito.mock
import org.mockito.Mockito.never
import org.mockito.Mockito.verify

class RealVideoGameReminderSchedulerTest {
    private val alarms: GameReminderAlarms = mock(GameReminderAlarms::class.java)
    private val telemetry: GameReminderTelemetry = mock(GameReminderTelemetry::class.java)

    private val scheduler = RealVideoGameReminderScheduler(alarms, FakeTimeProvider { NOW }, telemetry)

    private val game = ProductId.fromStoredValue("game.dot")

    @Test
    fun `an exact alarm is reported as scheduled for alarm delivery`() {
        withExactAlarms(allowed = true)

        scheduler.scheduleProductGameStart(game, START)

        verify(alarms).set(game, START, exact = true)
        verifyReported(GameReminderEvent.Scheduled(game, START, GameReminderDelivery.ALARM))
    }

    @Test
    fun `without exact alarms the reminder is reported as scheduled for notification delivery`() {
        withExactAlarms(allowed = false)

        scheduler.scheduleProductGameStart(game, START)

        verify(alarms).set(game, START, exact = false)
        verifyReported(GameReminderEvent.Scheduled(game, START, GameReminderDelivery.NOTIFICATION))
    }

    @Test
    fun `a start already past cancels the alarm and is reported as not scheduled`() {
        scheduler.scheduleProductGameStart(game, NOW)

        verify(alarms).cancel(game)
        verify(alarms, never()).set(any(), anyLong(), anyBoolean())
        verifyReported(GameReminderEvent.NotScheduled(game, NOW, GameReminderSkipReason.START_IN_PAST))
    }

    @Test
    fun `an alarm the OS refuses is reported as failed and still thrown`() {
        withExactAlarms(allowed = true)
        doThrow(SecurityException("revoked")).`when`(alarms).set(game, START, exact = true)

        assertThrows(SecurityException::class.java) { scheduler.scheduleProductGameStart(game, START) }

        verifyReported(GameReminderEvent.NotScheduled(game, START, GameReminderSkipReason.FAILED))
        verifyNotReported(GameReminderEvent.Scheduled(game, START, GameReminderDelivery.ALARM))
    }

    private fun withExactAlarms(allowed: Boolean) {
        whenever(alarms.canRingExactly()).thenReturn(allowed)
    }

    private fun verifyReported(event: GameReminderEvent) {
        verify(telemetry).report(event)
    }

    private fun verifyNotReported(event: GameReminderEvent) {
        verify(telemetry, never()).report(event)
    }

    private companion object {
        const val NOW = 1_000_000_000_000L
        const val START = NOW + 60_000
    }
}
