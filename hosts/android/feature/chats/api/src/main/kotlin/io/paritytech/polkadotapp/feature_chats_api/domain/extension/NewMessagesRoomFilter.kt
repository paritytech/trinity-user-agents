package io.paritytech.polkadotapp.feature_chats_api.domain.extension

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.extensionOrNull

interface NewMessagesRoomFilter {
    fun matches(chatId: ChatId): Boolean

    object Everything : NewMessagesRoomFilter {
        override fun matches(chatId: ChatId): Boolean {
            return true
        }
    }

    class Chat(val chatId: ChatId) : NewMessagesRoomFilter {
        override fun matches(chatId: ChatId): Boolean {
            return this.chatId == chatId
        }
    }

    class Chats(val chats: Set<ChatId>) : NewMessagesRoomFilter {
        override fun matches(chatId: ChatId): Boolean {
            return chatId in chats
        }
    }

    class AnyFromExtension(val extensionId: ChatExtensionId) : NewMessagesRoomFilter {
        override fun matches(chatId: ChatId): Boolean {
            val extensionIdFromChat = chatId.extensionOrNull() ?: return false
            return extensionIdFromChat.extensionId == extensionId
        }
    }
}
