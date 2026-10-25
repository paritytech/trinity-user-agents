package io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications

import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_videogame_impl.VideoGameNotificationPublisher
import io.paritytech.polkadotapp.feature_videogame_impl.data.notifications.VideoGameSettingsPreferences
import io.paritytech.polkadotapp.test_shared.FakeTimeProvider
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.anyLong
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.yield
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.clearInvocations
import org.mockito.Mockito.mock
import org.mockito.Mockito.never
import org.mockito.Mockito.times
import org.mockito.Mockito.verify
import kotlin.time.Duration.Companion.hours

class RealProductGameReminderTest {
    private val scheduler: VideoGameReminderScheduler = mock(VideoGameReminderScheduler::class.java)
    private val publisher: VideoGameNotificationPublisher = mock(VideoGameNotificationPublisher::class.java)
    private val calendar: ProductGameCalendar = mock(ProductGameCalendar::class.java)
    private val telemetry: GameReminderTelemetry = mock(GameReminderTelemetry::class.java)

    private var notificationsAllowed = false
    private var calendarAllowed = false

    // Answers only the next notifications ask, once completed.
    private var pendingNotificationsAnswer: CompletableDeferred<Boolean>? = null

    private val osAccess = object : ProductGameOsAccess {
        override suspend fun requestNotifications(): Result<Unit> {
            val pending = pendingNotificationsAnswer.also { pendingNotificationsAnswer = null }
            val allowed = pending?.await() ?: notificationsAllowed
            return if (allowed) Result.success(Unit) else Result.failure(IllegalStateException("refused"))
        }

        override suspend fun requestExactAlarms() = Unit

        override suspend fun requestCalendar(): Boolean = calendarAllowed
    }

    private val reminder = RealProductGameReminder(
        VideoGameSettingsPreferences(MapPreferences()),
        scheduler,
        publisher,
        calendar,
        osAccess,
        FakeTimeProvider { NOW },
        telemetry,
    )

    private val game = ProductId.fromStoredValue("game.dot")
    private val other = ProductId.fromStoredValue("acme.dot")

    @Test
    fun `schedule stores the game, arms its alarm and drops a posted notification`() = runBlocking<Unit> {
        withOsAllowing(notifications = true, calendar = false)

        val result = reminder.schedule(game, START)

        assertSuccess(result)
        assertEquals(listOf(ScheduledProductGame(game.value, START)), reminder.currentlyScheduled())
        verify(scheduler).scheduleProductGameStart(game, START)
        verify(publisher).cancelProductGameStartsSoonNotification(game)
        verify(calendar, never()).addGame(anyLong())
    }

    @Test
    fun `fails and stores nothing when notifications are refused`() = runBlocking<Unit> {
        withOsAllowing(notifications = false, calendar = true)

        val result = reminder.schedule(game, START)

        assertTrue(result.isFailure)
        assertTrue(reminder.currentlyScheduled().isEmpty())
        verifyReported(GameReminderEvent.NotScheduled(game, START, GameReminderSkipReason.NOT_AUTHORIZED))
        verify(scheduler, never()).scheduleProductGameStart(game, START)
        verify(calendar, never()).addGame(anyLong())
    }

    @Test
    fun `each product has its own scheduled game, soonest first`() = runBlocking<Unit> {
        withOsAllowing(notifications = true, calendar = false)
        reminder.schedule(game, START + 1)
        withOsAllowing(notifications = true, calendar = false)
        reminder.schedule(other, START)

        assertEquals(
            listOf(ScheduledProductGame(other.value, START), ScheduledProductGame(game.value, START + 1)),
            reminder.currentlyScheduled(),
        )
        verify(scheduler).scheduleProductGameStart(game, START + 1)
        verify(scheduler).scheduleProductGameStart(other, START)
    }

    @Test
    fun `a product replaces its own scheduled game`() = runBlocking<Unit> {
        withOsAllowing(notifications = true, calendar = false)
        reminder.schedule(game, START)
        withOsAllowing(notifications = true, calendar = false)

        reminder.schedule(game, START + 1)

        assertEquals(listOf(ScheduledProductGame(game.value, START + 1)), reminder.currentlyScheduled())
    }

    @Test
    fun `asks for calendar access and adds an event when the start is an hour or more away`() = runBlocking<Unit> {
        withOsAllowing(notifications = true, calendar = true)

        reminder.schedule(game, START)

        verify(calendar).addGame(START)
    }

    @Test
    fun `adds no event when calendar access is refused`() = runBlocking<Unit> {
        withOsAllowing(notifications = true, calendar = false)

        val result = reminder.schedule(game, START)

        assertSuccess(result)
        verify(calendar, never()).addGame(anyLong())
    }

    @Test
    fun `neither asks for calendar access nor adds an event a millisecond under an hour ahead`() = runBlocking<Unit> {
        withOsAllowing(notifications = true, calendar = true)

        reminder.schedule(game, NOW + 1.hours.inWholeMilliseconds - 1)

        verify(calendar, never()).addGame(anyLong())
    }

    @Test
    fun `adds a calendar event exactly an hour ahead`() = runBlocking<Unit> {
        withOsAllowing(notifications = true, calendar = true)

        reminder.schedule(game, NOW + 1.hours.inWholeMilliseconds)

        verify(calendar).addGame(NOW + 1.hours.inWholeMilliseconds)
    }

    @Test
    fun `a cancel made while a schedule waits on a prompt applies after it`() = runBlocking<Unit> {
        withOsAllowing(notifications = true, calendar = false)
        val notificationsAnswer = withPendingNotificationsPrompt()

        coroutineScope {
            launch { reminder.schedule(game, START) }
            yield()
            launch { reminder.cancel(game) }
            yield()
            notificationsAnswer.complete(true)
        }

        assertTrue(reminder.currentlyScheduled().isEmpty())
        verify(scheduler).cancelProductGameStart(game)
    }

    @Test
    fun `two schedules apply in call order when the first waits on a prompt`() = runBlocking<Unit> {
        withOsAllowing(notifications = true, calendar = false)
        val notificationsAnswer = withPendingNotificationsPrompt()

        coroutineScope {
            launch { reminder.schedule(game, START) }
            yield()
            launch { reminder.schedule(game, START + 1) }
            yield()
            notificationsAnswer.complete(true)
        }

        assertEquals(listOf(ScheduledProductGame(game.value, START + 1)), reminder.currentlyScheduled())
    }

    @Test
    fun `clearing a game does not wait on a pending prompt`() = runBlocking<Unit> {
        withOsAllowing(notifications = true, calendar = false)
        reminder.schedule(game, START)
        val notificationsAnswer = withPendingNotificationsPrompt()

        coroutineScope {
            launch { reminder.schedule(other, START) }
            yield()
            reminder.clear(ScheduledProductGame(game.value, START))

            assertTrue(reminder.currentlyScheduled().isEmpty())
            notificationsAnswer.complete(true)
        }
    }

    @Test
    fun `a cancel drops only that product's game, alarm and notification`() = runBlocking<Unit> {
        withOsAllowing(notifications = true, calendar = false)
        reminder.schedule(game, START)
        reminder.schedule(other, START + 1)

        reminder.cancel(other)
        assertEquals(listOf(ScheduledProductGame(game.value, START)), reminder.currentlyScheduled())
        verify(scheduler).cancelProductGameStart(other)
        verify(publisher, times(2)).cancelProductGameStartsSoonNotification(other)
        verify(scheduler, never()).cancelProductGameStart(game)
        verify(publisher).cancelProductGameStartsSoonNotification(game)

        reminder.cancel(game)

        assertTrue(reminder.currentlyScheduled().isEmpty())
        verify(scheduler).cancelProductGameStart(game)
        verify(publisher, times(2)).cancelProductGameStartsSoonNotification(game)
    }

    @Test
    fun `a product cancellation reports the cancelled game with its start`() = runBlocking<Unit> {
        withOsAllowing(notifications = true, calendar = false)
        reminder.schedule(game, START)

        reminder.cancelByProduct(game)

        verifyReported(GameReminderEvent.Cancelled(game, START))
    }

    @Test
    fun `a product cancellation with nothing scheduled reports nothing`() = runBlocking<Unit> {
        reminder.cancelByProduct(game)

        verifyNothingReported()
    }

    @Test
    fun `a host cancellation is not reported as a product cancellation`() = runBlocking<Unit> {
        withOsAllowing(notifications = true, calendar = false)
        reminder.schedule(game, START)

        reminder.cancel(game)

        assertTrue(reminder.currentlyScheduled().isEmpty())
        verifyNothingReported()
    }

    @Test
    fun `clear leaves a newer schedule alone`() = runBlocking<Unit> {
        withOsAllowing(notifications = true, calendar = false)
        reminder.schedule(game, START)
        val opened = ScheduledProductGame(game.value, START)
        reminder.schedule(game, START + 1)

        reminder.clear(opened)

        assertEquals(listOf(ScheduledProductGame(game.value, START + 1)), reminder.currentlyScheduled())
    }

    @Test
    fun `restore re-arms games whose start is ahead and drops those past the grace`() = runBlocking<Unit> {
        withOsAllowing(notifications = true, calendar = false)
        reminder.schedule(game, START)
        reminder.schedule(other, NOW - 30_000)
        clearInvocations(scheduler)

        reminder.restore()

        verify(scheduler).scheduleProductGameStart(game, START)
        verify(scheduler).cancelProductGameStart(other)
        assertEquals(listOf(ScheduledProductGame(game.value, START)), reminder.currentlyScheduled())
        assertNull(reminder.scheduledFor(other))
    }

    @Test
    fun `a game stays live until the grace after its start has run out`() {
        assertTrue(ScheduledProductGame(game.value, NOW - 29_999).isLiveAt(NOW))
        assertTrue(!ScheduledProductGame(game.value, NOW - 30_000).isLiveAt(NOW))
    }

    private fun withOsAllowing(notifications: Boolean, calendar: Boolean) {
        notificationsAllowed = notifications
        calendarAllowed = calendar
    }

    private fun withPendingNotificationsPrompt() =
        CompletableDeferred<Boolean>().also { pendingNotificationsAnswer = it }

    private fun verifyReported(event: GameReminderEvent) {
        verify(telemetry).report(event)
    }

    private fun verifyNothingReported() {
        verify(telemetry, never()).report(any())
    }

    private fun assertSuccess(result: Result<*>) {
        assertTrue("expected Result.success but was ${result.exceptionOrNull()}", result.isSuccess)
    }

    private companion object {
        const val NOW = 1_000_000_000_000L
        val START = NOW + 2.hours.inWholeMilliseconds
    }
}

private class MapPreferences : Preferences by mock(Preferences::class.java) {
    private val values = mutableMapOf<String, String>()

    override fun getString(field: String): String? = values[field]

    override fun putString(field: String, value: String?) {
        if (value == null) values.remove(field) else values[field] = value
    }
}
