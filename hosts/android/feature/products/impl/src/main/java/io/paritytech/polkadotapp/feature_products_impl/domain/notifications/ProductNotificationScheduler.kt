@file:OptIn(ExperimentalTime::class)

package io.paritytech.polkadotapp.feature_products_impl.domain.notifications

import android.app.AlarmManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.data.telemetry.ProductNotificationReason
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_impl.data.scheduledNotification.ScheduledProductNotification
import io.paritytech.polkadotapp.feature_products_impl.data.scheduledNotification.ScheduledProductNotificationRepository
import kotlinx.coroutines.CancellationException
import uniffi.truapi.ProductId
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

interface ProductNotificationScheduler {
    suspend fun schedule(
        productId: ProductId,
        text: String,
        deeplink: String?,
        scheduledAt: Instant,
    ): Result<NotificationId>

    suspend fun cancel(productId: ProductId, notificationId: NotificationId): Result<Unit>

    suspend fun cancelAllForProduct(productId: ProductId): Result<Unit>

    suspend fun restoreAll(): Result<Unit>
}

@Singleton
class RealProductNotificationScheduler @Inject constructor(
    @param:ApplicationContext private val appContext: Context,
    private val repository: ScheduledProductNotificationRepository,
    private val telemetry: ProductNotificationTelemetry,
) : ProductNotificationScheduler {
    private companion object {
        const val GLOBAL_SCHEDULE_CAPACITY = 64
    }

    private val alarmManager = appContext.getSystemService(AlarmManager::class.java)

    override suspend fun schedule(
        productId: ProductId,
        text: String,
        deeplink: String?,
        scheduledAt: Instant,
    ): Result<NotificationId> {
        if (repository.countAll() >= GLOBAL_SCHEDULE_CAPACITY) {
            telemetry.notScheduled(productId, notificationId = null, ProductNotificationReason.LIMIT_REACHED)
            return Result.failure(ProductPushNotificationException.ScheduleLimitReached)
        }

        val notificationId = NotificationId.generate(productId)
        if (repository.exists(productId, notificationId)) {
            telemetry.notScheduled(productId, notificationId, ProductNotificationReason.ID_COLLISION)
            return Result.failure(
                IllegalStateException("NotificationId collision for product ${productId.value}; retry the request")
            )
        }

        val scheduled = ScheduledProductNotification(
            productId = productId,
            notificationId = notificationId,
            text = text,
            deeplink = deeplink,
            scheduledAt = scheduledAt,
        )
        return registerAlarm(scheduled)
            .onFailure { telemetry.notScheduled(productId, notificationId, ProductNotificationReason.ADD_FAILED) }
            .mapCatching {
                try {
                    repository.insert(scheduled)
                } catch (error: CancellationException) {
                    throw error
                } catch (error: Exception) {
                    telemetry.notScheduled(productId, notificationId, ProductNotificationReason.STORE_FAILED)
                    throw error
                }
                telemetry.scheduled(scheduled)
                notificationId
            }
            .logFailure("Failed to schedule product notification")
    }

    override suspend fun cancel(productId: ProductId, notificationId: NotificationId): Result<Unit> = runCatching {
        val existed = repository.exists(productId, notificationId)
        alarmManager.cancel(buildAlarmPendingIntent(productId, notificationId))
        repository.delete(productId, notificationId)
        if (existed) telemetry.cancelled(productId, notificationId)
    }

    override suspend fun cancelAllForProduct(productId: ProductId): Result<Unit> = runCatching {
        val pending = repository.getAll().filter { it.productId == productId }
        pending.forEach { row ->
            alarmManager.cancel(buildAlarmPendingIntent(row.productId, row.notificationId))
        }
        repository.deleteAllByProduct(productId)
        pending.forEach { row -> telemetry.cancelled(row.productId, row.notificationId) }
    }

    override suspend fun restoreAll(): Result<Unit> = runCatching {
        repository.getAll().forEach { row ->
            registerAlarm(row)
        }
    }

    private fun registerAlarm(notification: ScheduledProductNotification): Result<Unit> {
        val pendingIntent = buildAlarmPendingIntent(notification.productId, notification.notificationId)

        return runCatching {
            alarmManager.setAndAllowWhileIdle(
                AlarmManager.RTC_WAKEUP,
                notification.scheduledAt.toEpochMilliseconds(),
                pendingIntent,
            )
        }
    }

    private fun buildAlarmPendingIntent(
        productId: ProductId,
        notificationId: NotificationId,
    ): PendingIntent {
        val intent = Intent(appContext, ProductNotificationReminderBroadcastReceiver::class.java).apply {
            action = ProductNotificationReminderBroadcastReceiver.ACTION_POST_PRODUCT_NOTIFICATION
            putExtra(ProductNotificationReminderBroadcastReceiver.EXTRA_PRODUCT_ID, productId.value)
            putExtra(ProductNotificationReminderBroadcastReceiver.EXTRA_NOTIFICATION_ID, notificationId.value)
        }

        return PendingIntent.getBroadcast(
            appContext,
            notificationId.value,
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
    }
}
