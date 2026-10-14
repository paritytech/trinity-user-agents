package io.paritytech.polkadotapp.feature_products_impl.domain.notifications

sealed class ProductPushNotificationException(message: String) : Exception(message) {
    data object ScheduleLimitReached : ProductPushNotificationException(
        "Schedule limit reached: the host already holds the maximum number of pending scheduled notifications"
    )
}
