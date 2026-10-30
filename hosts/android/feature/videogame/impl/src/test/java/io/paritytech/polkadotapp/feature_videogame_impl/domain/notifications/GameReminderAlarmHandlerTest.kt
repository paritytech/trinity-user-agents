package io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications

import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.data.app.AppLifecycleState
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_videogame_impl.VideoGameNotificationPublisher
import org.junit.Test

class GameReminderAlarmHandlerTest {
    private val publisher: VideoGameNotificationPublisher = mockk(relaxUnitFun = true)
    private val lifecycle: AppLifecycleObserver = mockk()
    private val reminder: RealProductGameReminder = mockk {
        every { scheduledFor(any()) } returns null
    }
    private val telemetry: GameReminderTelemetry = mockk(relaxUnitFun = true)

    private val handler = GameReminderAlarmHandler(publisher, lifecycle, reminder, telemetry)

    private val game = ProductId.fromStoredValue("game.dot")

    @Test
    fun `a background alarm for a held game is reported as fired and posts the notification`() {
        withAppIn(AppLifecycleState.BACKGROUND)
        withScheduledGame()

        handler.onAlarm(game, ringAlarm = true)

        verifyReported(
            GameReminderEvent.Fired(game, START, foreground = false, stillScheduled = true, delivery = GameReminderDelivery.ALARM)
        )
        verify(exactly = 1) { publisher.publishProductGameStartsSoonNotification(game, ringAlarm = true) }
    }

    @Test
    fun `a foreground alarm is reported as fired and posts nothing`() {
        withAppIn(AppLifecycleState.FOREGROUND)
        withScheduledGame()

        handler.onAlarm(game, ringAlarm = false)

        verifyReported(
            GameReminderEvent.Fired(game, START, foreground = true, stillScheduled = true, delivery = GameReminderDelivery.NOTIFICATION)
        )
        verifyNothingPosted()
    }

    @Test
    fun `an alarm for a game no longer held has no start and posts nothing`() {
        withAppIn(AppLifecycleState.BACKGROUND)

        handler.onAlarm(game, ringAlarm = true)

        verifyReported(
            GameReminderEvent.Fired(game, null, foreground = false, stillScheduled = false, delivery = GameReminderDelivery.ALARM)
        )
        verifyNothingPosted()
    }

    private fun withAppIn(state: AppLifecycleState) {
        every { lifecycle.getCurrentState() } returns state
    }

    private fun withScheduledGame() {
        every { reminder.scheduledFor(game) } returns ScheduledProductGame(game.value, START)
    }

    private fun verifyReported(event: GameReminderEvent) {
        verify(exactly = 1) { telemetry.report(event) }
    }

    private fun verifyNothingPosted() {
        verify(exactly = 0) { publisher.publishProductGameStartsSoonNotification(any(), any()) }
    }

    private companion object {
        const val START = 1_000_000_060_000L
    }
}
