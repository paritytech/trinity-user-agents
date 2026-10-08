package io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications

import io.mockk.Runs
import io.mockk.clearMocks
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.coVerifySequence
import io.mockk.just
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_videogame_impl.VideoGameNotificationPublisher
import io.paritytech.polkadotapp.feature_videogame_impl.data.notifications.VideoGameSettingsPreferences
import io.paritytech.polkadotapp.test_shared.FakeTimeProvider
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.time.Duration.Companion.hours

class RealProductGameReminderTest {
    private val scheduler: VideoGameReminderScheduler = mockk(relaxUnitFun = true)
    private val publisher: VideoGameNotificationPublisher = mockk(relaxUnitFun = true)
    private val calendar: ProductGameCalendar = mockk(relaxUnitFun = true)

    private var notificationsAllowed = false
    private var calendarAllowed = false

    // Answers only the next notifications ask, once completed.
    private var pendingNotificationsAnswer: CompletableDeferred<Boolean>? = null

    private val osAccess: ProductGameOsAccess = mockk {
        coEvery { requestNotifications() } coAnswers {
            val pending = pendingNotificationsAnswer.also { pendingNotificationsAnswer = null }
            val allowed = pending?.await() ?: notificationsAllowed
            if (allowed) Result.success(Unit) else Result.failure(IllegalStateException("refused"))
        }
        coEvery { requestExactAlarms() } just Runs
        coEvery { requestCalendar() } answers { calendarAllowed }
    }

    private val reminder = RealProductGameReminder(
        VideoGameSettingsPreferences(MapPreferences()),
        scheduler,
        publisher,
        calendar,
        osAccess,
        FakeTimeProvider { NOW },
    )

    private val game = ProductId.fromStoredValue("game.dot")
    private val other = ProductId.fromStoredValue("acme.dot")

    @Test
    fun `schedule stores the game, arms its alarm and drops a posted notification`() = runTest {
        withOsAllowing(notifications = true, calendar = false)

        val result = reminder.schedule(game, START)

        assertSuccess(result)
        assertEquals(listOf(ScheduledProductGame(game.value, START)), reminder.currentlyScheduled())
        verify { scheduler.scheduleProductGameStart(game, START) }
        verify { publisher.cancelProductGameStartsSoonNotification(game) }
        coVerify(exactly = 0) { calendar.addGame(any()) }
    }

    @Test
    fun `fails and stores nothing when notifications are refused`() = runTest {
        withOsAllowing(notifications = false, calendar = true)

        val result = reminder.schedule(game, START)

        assertTrue(result.isFailure)
        assertTrue(reminder.currentlyScheduled().isEmpty())
        coVerifySequence { osAccess.requestNotifications() }
        verify(exactly = 0) { scheduler.scheduleProductGameStart(game, START) }
        coVerify(exactly = 0) { calendar.addGame(any()) }
    }

    @Test
    fun `each product has its own scheduled game, soonest first`() = runTest {
        withOsAllowing(notifications = true, calendar = false)
        reminder.schedule(game, START + 1)
        withOsAllowing(notifications = true, calendar = false)
        reminder.schedule(other, START)

        assertEquals(
            listOf(ScheduledProductGame(other.value, START), ScheduledProductGame(game.value, START + 1)),
            reminder.currentlyScheduled(),
        )
        verify { scheduler.scheduleProductGameStart(game, START + 1) }
        verify { scheduler.scheduleProductGameStart(other, START) }
    }

    @Test
    fun `a product replaces its own scheduled game`() = runTest {
        withOsAllowing(notifications = true, calendar = false)
        reminder.schedule(game, START)
        withOsAllowing(notifications = true, calendar = false)

        reminder.schedule(game, START + 1)

        assertEquals(listOf(ScheduledProductGame(game.value, START + 1)), reminder.currentlyScheduled())
    }

    @Test
    fun `asks for calendar access and adds an event when the start is an hour or more away`() = runTest {
        withOsAllowing(notifications = true, calendar = true)

        reminder.schedule(game, START)

        coVerifySequence {
            osAccess.requestNotifications()
            osAccess.requestExactAlarms()
            osAccess.requestCalendar()
        }
        coVerify(exactly = 1) { calendar.addGame(START) }
    }

    @Test
    fun `adds no event when calendar access is refused`() = runTest {
        withOsAllowing(notifications = true, calendar = false)

        val result = reminder.schedule(game, START)

        assertSuccess(result)
        coVerify(exactly = 0) { calendar.addGame(any()) }
    }

    @Test
    fun `neither asks for calendar access nor adds an event a millisecond under an hour ahead`() = runTest {
        withOsAllowing(notifications = true, calendar = true)

        reminder.schedule(game, NOW + 1.hours.inWholeMilliseconds - 1)

        coVerifySequence {
            osAccess.requestNotifications()
            osAccess.requestExactAlarms()
        }
        coVerify(exactly = 0) { calendar.addGame(any()) }
    }

    @Test
    fun `adds a calendar event exactly an hour ahead`() = runTest {
        withOsAllowing(notifications = true, calendar = true)

        reminder.schedule(game, NOW + 1.hours.inWholeMilliseconds)

        coVerify(exactly = 1) { calendar.addGame(NOW + 1.hours.inWholeMilliseconds) }
    }

    @Test
    fun `a cancel made while a schedule waits on a prompt applies after it`() = runTest {
        withOsAllowing(notifications = true, calendar = false)
        val notificationsAnswer = withPendingNotificationsPrompt()

        launch { reminder.schedule(game, START) }
        testScheduler.runCurrent()
        launch { reminder.cancel(game) }
        testScheduler.runCurrent()
        notificationsAnswer.complete(true)
        testScheduler.advanceUntilIdle()

        assertTrue(reminder.currentlyScheduled().isEmpty())
        verify { scheduler.cancelProductGameStart(game) }
    }

    @Test
    fun `two schedules apply in call order when the first waits on a prompt`() = runTest {
        withOsAllowing(notifications = true, calendar = false)
        val notificationsAnswer = withPendingNotificationsPrompt()

        launch { reminder.schedule(game, START) }
        testScheduler.runCurrent()
        launch { reminder.schedule(game, START + 1) }
        testScheduler.runCurrent()
        notificationsAnswer.complete(true)
        testScheduler.advanceUntilIdle()

        assertEquals(listOf(ScheduledProductGame(game.value, START + 1)), reminder.currentlyScheduled())
    }

    @Test
    fun `clearing a game does not wait on a pending prompt`() = runTest {
        withOsAllowing(notifications = true, calendar = false)
        reminder.schedule(game, START)
        val notificationsAnswer = withPendingNotificationsPrompt()

        launch { reminder.schedule(other, START) }
        testScheduler.runCurrent()
        reminder.clear(ScheduledProductGame(game.value, START))

        assertTrue(reminder.currentlyScheduled().isEmpty())
        notificationsAnswer.complete(true)
    }

    @Test
    fun `a cancel drops only that product's game, alarm and notification`() = runTest {
        withOsAllowing(notifications = true, calendar = false)
        reminder.schedule(game, START)
        reminder.schedule(other, START + 1)

        reminder.cancel(other)
        assertEquals(listOf(ScheduledProductGame(game.value, START)), reminder.currentlyScheduled())
        verify { scheduler.cancelProductGameStart(other) }
        verify(exactly = 2) { publisher.cancelProductGameStartsSoonNotification(other) }
        verify(exactly = 0) { scheduler.cancelProductGameStart(game) }
        verify(exactly = 1) { publisher.cancelProductGameStartsSoonNotification(game) }

        reminder.cancel(game)

        assertTrue(reminder.currentlyScheduled().isEmpty())
        verify { scheduler.cancelProductGameStart(game) }
        verify(exactly = 2) { publisher.cancelProductGameStartsSoonNotification(game) }
    }

    @Test
    fun `clear leaves a newer schedule alone`() = runTest {
        withOsAllowing(notifications = true, calendar = false)
        reminder.schedule(game, START)
        val opened = ScheduledProductGame(game.value, START)
        reminder.schedule(game, START + 1)

        reminder.clear(opened)

        assertEquals(listOf(ScheduledProductGame(game.value, START + 1)), reminder.currentlyScheduled())
    }

    @Test
    fun `restore re-arms games whose start is ahead and drops those past the grace`() = runTest {
        withOsAllowing(notifications = true, calendar = false)
        reminder.schedule(game, START)
        reminder.schedule(other, NOW - 30_000)
        clearMocks(scheduler, answers = false)

        reminder.restore()

        verify { scheduler.scheduleProductGameStart(game, START) }
        verify { scheduler.cancelProductGameStart(other) }
        assertEquals(listOf(ScheduledProductGame(game.value, START)), reminder.currentlyScheduled())
        assertNull(reminder.scheduledFor(other))
    }

    @Test
    fun `a game stays live until the grace after its start has run out`() = runTest {
        assertTrue(ScheduledProductGame(game.value, NOW - 29_999).isLiveAt(NOW))
        assertTrue(!ScheduledProductGame(game.value, NOW - 30_000).isLiveAt(NOW))
    }

    private fun withOsAllowing(notifications: Boolean, calendar: Boolean) {
        notificationsAllowed = notifications
        calendarAllowed = calendar
    }

    private fun withPendingNotificationsPrompt() =
        CompletableDeferred<Boolean>().also { pendingNotificationsAnswer = it }

    private fun assertSuccess(result: Result<*>) {
        assertTrue("expected Result.success but was ${result.exceptionOrNull()}", result.isSuccess)
    }

    private companion object {
        const val NOW = 1_000_000_000_000L
        val START = NOW + 2.hours.inWholeMilliseconds
    }
}

private class MapPreferences : Preferences by mockk() {
    private val values = mutableMapOf<String, String>()

    override fun getString(field: String): String? = values[field]

    override fun putString(field: String, value: String?) {
        if (value == null) values.remove(field) else values[field] = value
    }
}
