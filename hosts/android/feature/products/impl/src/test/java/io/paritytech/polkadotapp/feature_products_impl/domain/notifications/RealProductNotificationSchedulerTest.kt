@file:OptIn(ExperimentalTime::class)

package io.paritytech.polkadotapp.feature_products_impl.domain.notifications

import android.app.AlarmManager
import android.content.Context
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.data.scheduledNotification.ScheduledProductNotificationRepository
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

class RealProductNotificationSchedulerTest {
    private val context: Context = mockk()
    private val alarmManager: AlarmManager = mockk()
    private val repository: ScheduledProductNotificationRepository = mockk()

    private val productId = ProductId.fromStoredValue("acme.dot")
    private val scheduledAt = Instant.fromEpochMilliseconds(2_000_000_000_000L)

    private lateinit var scheduler: RealProductNotificationScheduler

    @Before
    fun setUp() {
        every { context.getSystemService(AlarmManager::class.java) } returns alarmManager
        scheduler = RealProductNotificationScheduler(context, repository)
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

    private suspend fun withPendingNotificationsCount(count: Int) {
        coEvery { repository.countAll() } returns count
    }

    private suspend fun withCollisionForProduct(productId: ProductId) {
        coEvery { repository.exists(productId, any()) } returns true
    }

    private suspend fun scheduleNotification(): Result<NotificationId> {
        return scheduler.schedule(productId, "hello", deeplink = null, scheduledAt = scheduledAt)
    }
}
