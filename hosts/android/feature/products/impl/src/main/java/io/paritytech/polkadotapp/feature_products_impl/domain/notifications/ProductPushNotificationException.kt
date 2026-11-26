package io.paritytech.polkadotapp.feature_products_impl.domain.notifications

import io.paritytech.polkadotapp.common.data.telemetry.ProductNotificationReason
import uniffi.truapi.ProductId

/**
 * Why a product notification was not booked.
 *
 * A wrapper keeps the message of its cause, since the product reads that message.
 */
sealed class ProductPushNotificationException(
    val reason: ProductNotificationReason,
    val notificationId: NotificationId?,
    message: String,
    cause: Throwable? = null,
) : Exception(message, cause) {
    data object ScheduleLimitReached : ProductPushNotificationException(
        ProductNotificationReason.LIMIT_REACHED,
        notificationId = null,
        "Schedule limit reached: the host already holds the maximum number of pending scheduled notifications",
    )

    class ReadFailed(cause: Throwable) : ProductPushNotificationException(
        ProductNotificationReason.READ_FAILED,
        notificationId = null,
        cause.message.orEmpty(),
        cause,
    )

    class IdCollision(productId: ProductId, notificationId: NotificationId) : ProductPushNotificationException(
        ProductNotificationReason.ID_COLLISION,
        notificationId,
        "NotificationId collision for product ${productId.value}; retry the request",
    )

    class AddFailed(notificationId: NotificationId, cause: Throwable) : ProductPushNotificationException(
        ProductNotificationReason.ADD_FAILED,
        notificationId,
        cause.message.orEmpty(),
        cause,
    )

    class StoreFailed(notificationId: NotificationId, cause: Throwable) : ProductPushNotificationException(
        ProductNotificationReason.STORE_FAILED,
        notificationId,
        cause.message.orEmpty(),
        cause,
    )
}
