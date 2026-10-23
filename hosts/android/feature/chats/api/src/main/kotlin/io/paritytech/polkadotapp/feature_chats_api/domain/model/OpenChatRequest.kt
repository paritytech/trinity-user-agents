package io.paritytech.polkadotapp.feature_chats_api.domain.model

sealed class OpenChatRequest {
    class ExistingChat(val chatId: ChatId) : OpenChatRequest()
}

fun OpenChatRequest.computeChatId(): ChatId {
    return when (this) {
        is OpenChatRequest.ExistingChat -> chatId
    }
}
