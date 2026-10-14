@file:OptIn(ExperimentalTime::class)

package io.paritytech.polkadotapp.feature_products_impl.domain.notifications

import android.app.AlarmManager
import android.content.Context
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.data.scheduledNotification.ScheduledProductNotificationRepository
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.eq
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.mockito.Mockito.mock
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

class RealProductNotificationSchedulerTest {
    private val context: Context = mock()
    private val alarmManager: AlarmManager = mock()
    private val repository: ScheduledProductNotificationRepository = mock()

    private val productId = ProductId.fromStoredValue("acme.dot")
    private val scheduledAt = Instant.fromEpochMilliseconds(2_000_000_000_000L)

    private lateinit var scheduler: RealProductNotificationScheduler

    @Before
    fun setUp() {
        whenever(context.getSystemService(AlarmManager::class.java)).thenReturn(alarmManager)
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
        whenever(repository.countAll()).thenReturn(count)
    }

    private suspend fun withCollisionForProduct(productId: ProductId) {
        whenever(repository.exists(eq(productId), any())).thenReturn(true)
    }

    private suspend fun scheduleNotification(): Result<NotificationId> {
        return scheduler.schedule(productId, "hello", deeplink = null, scheduledAt = scheduledAt)
    }
}
