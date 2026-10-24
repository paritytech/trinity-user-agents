package io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications

import io.paritytech.polkadotapp.common.data.app.AppLifecycleState
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_videogame_impl.VideoGameNotificationPublisher
import io.paritytech.polkadotapp.feature_videogame_impl.data.notifications.GameReminderDelivery
import io.paritytech.polkadotapp.feature_videogame_impl.data.notifications.GameReminderEvent
import io.paritytech.polkadotapp.feature_videogame_impl.data.notifications.GameReminderTelemetry
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.whenever
import org.junit.Test
import org.mockito.Mockito.anyBoolean
import org.mockito.Mockito.anyLong
import org.mockito.Mockito.mock
import org.mockito.Mockito.never
import org.mockito.Mockito.verify

class GameReminderAlarmHandlerTest {
    private val publisher: VideoGameNotificationPublisher = mock(VideoGameNotificationPublisher::class.java)
    private val lifecycle: AppLifecycleObserver = mock(AppLifecycleObserver::class.java)
    private val reminder: RealProductGameReminder = mock(RealProductGameReminder::class.java)
    private val telemetry: GameReminderTelemetry = mock(GameReminderTelemetry::class.java)

    private val handler = GameReminderAlarmHandler(publisher, lifecycle, reminder, telemetry)

    private val game = ProductId.fromStoredValue("game.dot")

    @Test
    fun `a background alarm for a held game is reported as fired and posts the notification`() {
        withAppIn(AppLifecycleState.BACKGROUND)
        withScheduledGame()

        handler.onAlarm(game, START, ringAlarm = true)

        verifyReported(
            GameReminderEvent.Fired(game, START, foreground = false, stillScheduled = true, delivery = GameReminderDelivery.ALARM)
        )
        verify(publisher).publishProductGameStartsSoonNotification(game, START, ringAlarm = true)
    }

    @Test
    fun `a foreground alarm is reported as fired and posts nothing`() {
        withAppIn(AppLifecycleState.FOREGROUND)
        withScheduledGame()

        handler.onAlarm(game, START, ringAlarm = false)

        verifyReported(
            GameReminderEvent.Fired(game, START, foreground = true, stillScheduled = true, delivery = GameReminderDelivery.NOTIFICATION)
        )
        verifyNothingPosted()
    }

    @Test
    fun `an alarm for a game no longer held keeps the start from the intent and posts nothing`() {
        withAppIn(AppLifecycleState.BACKGROUND)

        handler.onAlarm(game, START, ringAlarm = true)

        verifyReported(
            GameReminderEvent.Fired(game, START, foreground = false, stillScheduled = false, delivery = GameReminderDelivery.ALARM)
        )
        verifyNothingPosted()
    }

    @Test
    fun `a dismissal is reported`() {
        handler.onDismissed(game, START)

        verifyReported(GameReminderEvent.Dismissed(game, START))
    }

    private fun withAppIn(state: AppLifecycleState) {
        whenever(lifecycle.getCurrentState()).thenReturn(state)
    }

    private fun withScheduledGame() {
        whenever(reminder.scheduledFor(game)).thenReturn(ScheduledProductGame(game.value, START))
    }

    private fun verifyReported(event: GameReminderEvent) {
        verify(telemetry).report(event)
    }

    private fun verifyNothingPosted() {
        verify(publisher, never()).publishProductGameStartsSoonNotification(any(), anyLong(), anyBoolean())
    }

    private companion object {
        const val START = 1_000_000_060_000L
    }
}
