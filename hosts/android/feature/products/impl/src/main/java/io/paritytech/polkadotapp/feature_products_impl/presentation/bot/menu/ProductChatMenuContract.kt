package io.paritytech.polkadotapp.feature_products_impl.presentation.bot.menu

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import kotlinx.coroutines.flow.StateFlow

enum class ProductChatMenuPage {
    MAIN,
    REMOVE_CONFIRMATION
}

interface ProductChatMenuContract {
    val currentPage: StateFlow<ProductChatMenuPage>
    fun onOpenAppClick(productId: ProductId)
    fun onRemoveChatClick()
    fun onRemoveChatConfirmed(productId: ProductId, chatId: ChatId)
    fun onMenuDismissed()
}
