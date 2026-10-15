package io.paritytech.polkadotapp.feature_products_impl.presentation.bot.menu

import io.paritytech.polkadotapp.feature_chats_api.domain.ChatMuteDuration
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRoomUserStateModel
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.StateFlow

enum class ProductChatMenuPage {
    MAIN,
    MUTE_DURATION,
    REMOVE_CONFIRMATION
}

interface ProductChatMenuContract {
    val currentPage: StateFlow<ProductChatMenuPage>

    /** What the user set for the room: the menu offers Unmute and Unarchive from it (07-LATER §6.2). */
    fun userState(chatId: ChatId): Flow<ChatRoomUserStateModel>
    fun onOpenAppClick(productId: ProductId)

    /** A muted room unmutes at once. Another room opens the durations of a mute. */
    fun onMuteClick(chatId: ChatId, isMuted: Boolean)
    fun onMuteDurationSelected(chatId: ChatId, duration: ChatMuteDuration)
    fun onArchiveClick(chatId: ChatId, isArchived: Boolean)
    fun onRemoveChatClick()
    fun onRemoveChatConfirmed(productId: ProductId, chatId: ChatId)
    fun onMenuDismissed()
}
