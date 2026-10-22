package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.novasama.substrate_sdk_android.extensions.fromHex
import io.parity.truapi.PushHostBridge
import io.paritytech.polkadotapp.common.data.os.OperatingSystem
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ReservedProductIds
import io.paritytech.polkadotapp.tools_push_notifications_api.PushNotificationsHelper
import uniffi.truapi.HostChatPushRoute
import uniffi.truapi.HostPushDelivery
import uniffi.truapi.HostPushService
import uniffi.truapi.HostPushTokens
import javax.inject.Inject

/**
 * The push service of products on Android: the FCM token of this device, the push relay of the backend through the
 * app's own relay client (its JWT never reaches a product), and the chat push routes the chat product gives.
 */
class AppPushHostBridge @Inject constructor(
    private val pushNotifications: PushNotificationsHelper,
    private val chatRoutes: ProductChatPushRoutes,
) : PushHostBridge {
    override suspend fun sendPush(productId: String, delivery: HostPushDelivery): Boolean =
        pushNotifications.sendNotify(
            platformToken = delivery.token,
            operatingSystem = if (delivery.service == HostPushService.APNS) OperatingSystem.IOS else OperatingSystem.ANDROID,
            pushId = delivery.pushId.fromHex(),
            encryptedMessage = delivery.message.fromHex(),
            isVoIP = delivery.voip,
        ).isSuccess

    override suspend fun pushTokens(): HostPushTokens =
        HostPushTokens(service = HostPushService.FCM, token = pushNotifications.getCurrentToken(), voipToken = null)

    // Only the chat product shows its pushes as person chats; another product's routes would claim pushes it never
    // showed before the chat product.
    override suspend fun setChatPushRoutes(productId: String, routes: List<HostChatPushRoute>) {
        if (!ReservedProductIds.isChat(ProductId.fromStoredValue(productId))) return
        chatRoutes.replace(productId, routes)
    }
}
