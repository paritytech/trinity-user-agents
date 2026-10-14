package io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot

import io.paritytech.polkadotapp.feature_chats_api.domain.model.filterCustomContents

interface ChatBotMessageProcessor {
    context(chatBotContext: ChatBotContext)
    fun launchSendingMessages()
}

context(chatBotContext: ChatBotContext)
suspend inline fun <reified T> messageWasSent(): Boolean {
    return chatBotContext.getPersistedMessages()
        .filterCustomContents<T>()
        .isNotEmpty()
}
