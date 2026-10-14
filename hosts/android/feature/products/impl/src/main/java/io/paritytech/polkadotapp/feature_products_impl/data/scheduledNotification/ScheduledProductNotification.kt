@file:OptIn(ExperimentalTime::class)

package io.paritytech.polkadotapp.feature_products_impl.data.scheduledNotification

import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.notifications.NotificationId
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

class ScheduledProductNotification(
    val productId: ProductId,
    val notificationId: NotificationId,
    val text: String,
    val deeplink: String?,
    val scheduledAt: Instant,
)
