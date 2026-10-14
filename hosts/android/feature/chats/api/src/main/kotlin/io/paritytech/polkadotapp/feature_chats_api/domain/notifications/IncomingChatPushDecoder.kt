package io.paritytech.polkadotapp.feature_chats_api.domain.notifications

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact

interface IncomingChatPushDecoder {
    suspend fun decode(data: Map<String, String>): Result<DecodedChatPush>

    companion object {
        const val PUSH_ID_KEY = "pushId"
        const val MESSAGE_KEY = "message"
    }
}

data class DecodedChatPush(
    val contact: Contact,
    val chatId: ChatId,
    val content: ChatPushContent,
)

sealed interface ChatPushContent {
    val messageId: String

    class Full(val message: ChatMessage) : ChatPushContent {
        override val messageId: String get() = message.id
    }

    /**
     * Enough to display a notification, but not the message itself: must never be persisted.
     */
    class Stripped(
        override val messageId: String,
        val timestamp: Long,
        val content: StrippedChatPushContent,
    ) : ChatPushContent
}

sealed interface StrippedChatPushContent {
    class Regular(val content: ChatMessage.Content) : StrippedChatPushContent

    class CoinagePayment(val totalValue: Balance) : StrippedChatPushContent
}
