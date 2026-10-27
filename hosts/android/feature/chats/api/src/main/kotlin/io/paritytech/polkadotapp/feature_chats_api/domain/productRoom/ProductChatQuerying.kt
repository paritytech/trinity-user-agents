package io.paritytech.polkadotapp.feature_chats_api.domain.productRoom

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId

/**
 * The queries that a room asks of its product. A failure is a [ProductChatQueryException]. The timeouts are the
 * ones of Chat Extension v2: 10 s for a page.
 */
interface ProductChatQuerying {
    suspend fun messagesPage(chatId: ChatId, anchor: ChatPageAnchorModel, pageSize: Int): Result<ProductMessagesPage>
}
