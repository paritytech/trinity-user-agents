@file:OptIn(ExperimentalTime::class)

package io.paritytech.polkadotapp.feature_products_impl.domain.notifications

import android.app.AlarmManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import io.mockk.Runs
import io.mockk.coEvery
import io.mockk.every
import io.mockk.just
import io.mockk.mockk
import io.mockk.mockkConstructor
import io.mockk.mockkStatic
import io.mockk.slot
import io.mockk.unmockkAll
import io.paritytech.polkadotapp.feature_products_impl.data.scheduledNotification.ScheduledProductNotification
import io.paritytech.polkadotapp.feature_products_impl.data.scheduledNotification.ScheduledProductNotificationRepository
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.eq
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.mockito.Mockito.mock
import uniffi.truapi.ProductId
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

class RealProductNotificationSchedulerTest {
    private val context: Context = mock()
    private val alarmManager: AlarmManager = mock()
    private val repository: ScheduledProductNotificationRepository = mock()

    private val productId = ProductId.parse("acme.dot").getOrThrow()
    private val scheduledAt = Instant.fromEpochMilliseconds(2_000_000_000_000L)

    private lateinit var scheduler: RealProductNotificationScheduler

    @Before
    fun setUp() {
        whenever(context.getSystemService(AlarmManager::class.java)).thenReturn(alarmManager)
        scheduler = RealProductNotificationScheduler(context, repository)
    }

    @After
    fun tearDown() {
        unmockkAll()
    }

    @Test
    fun `returns ScheduleLimitReached when capacity is full`() = runBlocking {
        withPendingNotificationsCount(64)

        val result = scheduleNotification()

        assertTrue(result.isFailure)
        assertTrue(result.exceptionOrNull() is ProductPushNotificationException.ScheduleLimitReached)
    }

    @Test
    fun `returns ScheduleLimitReached when capacity is exceeded`() = runBlocking {
        withPendingNotificationsCount(65)

        val result = scheduleNotification()

        assertTrue(result.isFailure)
        assertTrue(result.exceptionOrNull() is ProductPushNotificationException.ScheduleLimitReached)
    }

    @Test
    fun `returns failure on notification id collision`() = runBlocking {
        withPendingNotificationsCount(0)
        withCollisionForProduct(productId)

        val result = scheduleNotification()

        assertTrue(result.isFailure)
    }

    /** The alarm publishes from the stored row, so the title a product named must be in it. */
    @Test
    fun `stores the title with the scheduled notification`() = runBlocking {
        val stored = slot<ScheduledProductNotification>()
        val repository = mockk<ScheduledProductNotificationRepository> {
            coEvery { countAll() } returns 0
            coEvery { exists(productId, any()) } returns false
            coEvery { insert(capture(stored)) } just Runs
        }
        withAlarmIntents()

        RealProductNotificationScheduler(context, repository)
            .schedule(productId, title = "Order shipped", "hello", deeplink = null, scheduledAt = scheduledAt)

        assertEquals("Order shipped", stored.captured.title)
    }

    private fun withAlarmIntents() {
        mockkConstructor(Intent::class)
        every { anyConstructed<Intent>().setAction(any()) } answers { self as Intent }
        every { anyConstructed<Intent>().putExtra(any<String>(), any<String>()) } answers { self as Intent }
        every { anyConstructed<Intent>().putExtra(any<String>(), any<Int>()) } answers { self as Intent }
        mockkStatic(PendingIntent::class)
        every { PendingIntent.getBroadcast(any(), any(), any(), any()) } returns mockk()
    }

    private suspend fun withPendingNotificationsCount(count: Int) {
        whenever(repository.countAll()).thenReturn(count)
    }

    private suspend fun withCollisionForProduct(productId: ProductId) {
        whenever(repository.exists(eq(productId), any())).thenReturn(true)
    }

    private suspend fun scheduleNotification(): Result<NotificationId> {
        return scheduler.schedule(productId, title = null, "hello", deeplink = null, scheduledAt = scheduledAt)
    }
}
