package io.paritytech.polkadotapp.feature_products_impl.domain.notifications

import io.paritytech.polkadotapp.common.data.app.AppLifecycleState
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatActiveTracker
import io.paritytech.polkadotapp.feature_chats_api.domain.ReadOnlyChatRoomRepository
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.toChatExtensionId
import io.paritytech.polkadotapp.feature_products_api.model.toChatId
import javax.inject.Inject

/** How the host shows one notification of a product. */
sealed interface ProductNotificationPresentation {
    /** The room of the notification is on screen, so the notification does not show. */
    data object Hidden : ProductNotificationPresentation

    /** The notification shows, with [title] as its title. A null title keeps the default title. */
    data class Shown(val title: String?) : ProductNotificationPresentation
}

/**
 * Shows a notification of a product that opens a room of the same product as a person chat notification shows
 * (Chat Extension v2 "Room notifications"): the room name is the title, and nothing shows while the app is in the
 * foreground with that room on screen. A notification that opens no room of the product shows as before.
 */
class ProductRoomNotificationPolicy @Inject constructor(
    private val rooms: ReadOnlyChatRoomRepository,
    private val chatActiveTracker: ChatActiveTracker,
    private val appLifecycleObserver: AppLifecycleObserver,
) {
    suspend fun presentation(productId: ProductId, deeplink: String?): ProductNotificationPresentation {
        val roomId = deeplink?.let { productRoomOfLink(productId, it) }
            ?: return ProductNotificationPresentation.Shown(title = null)
        val chatId = ProductChatIdParameter(roomId).toChatId(productId.toChatExtensionId())

        val onScreen = appLifecycleObserver.getCurrentState() == AppLifecycleState.FOREGROUND &&
            chatActiveTracker.getActive() == chatId
        if (onScreen) return ProductNotificationPresentation.Hidden

        val name = rooms.getRoomMetadata(chatId).name?.takeIf { it.isNotBlank() }
        return ProductNotificationPresentation.Shown(title = name)
    }
}
