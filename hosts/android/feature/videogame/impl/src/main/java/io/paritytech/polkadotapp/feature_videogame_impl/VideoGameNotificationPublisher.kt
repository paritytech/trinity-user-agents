package io.paritytech.polkadotapp.feature_videogame_impl

import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.feature_products_api.model.ProductId

interface VideoGameNotificationPublisher {
    fun publishRegistrationOpenedNotification(timestamp: Timestamp)
    fun publishWaitingRoomAvailableNotification()
    fun publishGameAboutToStartNotification()
    fun publishGameStartsSoonNotification()
    fun publishProductGameStartsSoonNotification(productId: ProductId, ringAlarm: Boolean)
    fun cancelProductGameStartsSoonNotification(productId: ProductId)
    fun cancelProductGameStartNotifications()
    fun cancelGameStartNotifications()
}
