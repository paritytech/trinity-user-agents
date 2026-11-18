@file:OptIn(ExperimentalTime::class)

package io.paritytech.polkadotapp.feature_products_impl.data.scheduledNotification

import io.mockk.coEvery
import io.mockk.mockk
import io.mockk.slot
import io.paritytech.polkadotapp.database.dao.ScheduledProductNotificationDao
import io.paritytech.polkadotapp.database.model.ScheduledProductNotificationLocal
import io.paritytech.polkadotapp.feature_products_impl.domain.notifications.NotificationId
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi.ProductId
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

class RealScheduledProductNotificationRepositoryTest {
    private val dao: ScheduledProductNotificationDao = mockk(relaxed = true)
    private val repository = RealScheduledProductNotificationRepository(dao)

    private val productId = ProductId.parse("acme.dot").getOrThrow()
    private val notificationId = NotificationId(7)

    /**
     * A scheduled notification is published from its stored row, so a title lost on the way to
     * the database would leave the alarm heading the notification with the product id instead.
     */
    @Test
    fun `a title survives the stored row`() = runBlocking {
        val stored = slot<ScheduledProductNotificationLocal>()
        coEvery { dao.insert(capture(stored)) } returns Unit
        coEvery { dao.find(productId, notificationId.value) } answers { stored.captured }

        repository.insert(
            ScheduledProductNotification(
                productId = productId,
                notificationId = notificationId,
                title = "Order shipped",
                text = "hello",
                deeplink = null,
                scheduledAt = Instant.fromEpochMilliseconds(2_000_000_000_000L),
            )
        )

        assertEquals("Order shipped", repository.find(productId, notificationId)?.title)
    }
}
