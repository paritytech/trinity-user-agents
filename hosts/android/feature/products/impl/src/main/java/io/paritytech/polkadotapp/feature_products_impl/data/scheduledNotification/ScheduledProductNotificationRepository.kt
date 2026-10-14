@file:OptIn(ExperimentalTime::class)

package io.paritytech.polkadotapp.feature_products_impl.data.scheduledNotification

import io.paritytech.polkadotapp.database.dao.ScheduledProductNotificationDao
import io.paritytech.polkadotapp.database.model.ScheduledProductNotificationLocal
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.notifications.NotificationId
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

interface ScheduledProductNotificationRepository {
    suspend fun insert(notification: ScheduledProductNotification)

    suspend fun getAll(): List<ScheduledProductNotification>

    suspend fun find(productId: ProductId, notificationId: NotificationId): ScheduledProductNotification?

    suspend fun countAll(): Int

    suspend fun exists(productId: ProductId, notificationId: NotificationId): Boolean

    suspend fun delete(productId: ProductId, notificationId: NotificationId)

    suspend fun deleteAllByProduct(productId: ProductId)
}

@Singleton
class RealScheduledProductNotificationRepository @Inject constructor(
    private val dao: ScheduledProductNotificationDao,
) : ScheduledProductNotificationRepository {
    override suspend fun insert(notification: ScheduledProductNotification) {
        dao.insert(notification.toLocal())
    }

    override suspend fun getAll(): List<ScheduledProductNotification> {
        return dao.getAll().map { it.toDomain() }
    }

    override suspend fun find(
        productId: ProductId,
        notificationId: NotificationId,
    ): ScheduledProductNotification? {
        return dao.find(productId.value, notificationId.value)?.toDomain()
    }

    override suspend fun countAll(): Int {
        return dao.countAll()
    }

    override suspend fun exists(productId: ProductId, notificationId: NotificationId): Boolean {
        return dao.exists(productId.value, notificationId.value)
    }

    override suspend fun delete(productId: ProductId, notificationId: NotificationId) {
        dao.delete(productId.value, notificationId.value)
    }

    override suspend fun deleteAllByProduct(productId: ProductId) {
        dao.deleteAllByProduct(productId.value)
    }

    private fun ScheduledProductNotification.toLocal(): ScheduledProductNotificationLocal {
        return ScheduledProductNotificationLocal(
            productId = productId.value,
            notificationId = notificationId.value,
            text = text,
            deeplink = deeplink,
            scheduledAtMs = scheduledAt.toEpochMilliseconds(),
        )
    }

    private fun ScheduledProductNotificationLocal.toDomain(): ScheduledProductNotification {
        return ScheduledProductNotification(
            productId = ProductId.fromStoredValue(productId),
            notificationId = NotificationId(notificationId),
            text = text,
            deeplink = deeplink,
            scheduledAt = Instant.fromEpochMilliseconds(scheduledAtMs),
        )
    }
}
