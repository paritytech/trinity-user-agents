package io.paritytech.polkadotapp.feature_videogame_impl

import uniffi.truapi.ProductId

interface VideoGameNotificationPublisher {
    fun publishProductGameStartsSoonNotification(productId: ProductId, ringAlarm: Boolean)
    fun cancelProductGameStartsSoonNotification(productId: ProductId)
    fun cancelProductGameStartNotifications()
}
