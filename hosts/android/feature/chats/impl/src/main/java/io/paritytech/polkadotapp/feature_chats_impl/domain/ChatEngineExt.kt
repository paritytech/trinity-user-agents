package io.paritytech.polkadotapp.feature_chats_impl.domain

import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatMessageRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatMessageRendererId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId

suspend fun ChatEngine.getCustomMessageRenderer(chatId: ChatId, rendererId: CustomChatMessageRendererId): CustomChatMessageRenderer<*>? {
    return getCustomRenderersForChat(chatId)[rendererId]
}
