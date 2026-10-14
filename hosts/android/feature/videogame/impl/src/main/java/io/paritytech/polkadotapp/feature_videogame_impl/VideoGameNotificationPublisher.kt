package io.paritytech.polkadotapp.feature_videogame_impl

import io.paritytech.polkadotapp.feature_products_api.model.ProductId

interface VideoGameNotificationPublisher {
    fun publishProductGameStartsSoonNotification(productId: ProductId, ringAlarm: Boolean)
    fun cancelProductGameStartsSoonNotification(productId: ProductId)
    fun cancelProductGameStartNotifications()
}
