package io.paritytech.polkadotapp.feature_chats_api.domain.model

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.startsWith

@JvmInline
value class ChatId private constructor(val value: DataByteArray) {
    companion object {
        fun fromRawValue(id: ByteArray): ChatId {
            return ChatId(id.toDataByteArray())
        }

        fun forExtension(extensionId: ChatExtensionId): ChatId {
            return ChatId("$EXTENSION_PREFIX$extensionId".encodeToByteArray().toDataByteArray())
        }

        fun forExtensionRoom(extensionId: ChatExtensionId, subRoomId: String): ChatId {
            return ChatId("$EXTENSION_PREFIX$extensionId:$subRoomId".encodeToByteArray().toDataByteArray())
        }

        fun fromContact(contactId: AccountId): ChatId {
            return ChatId(contactId)
        }

        /**
         * Kept for backward compatibility during transition. Delegates to [forExtension].
         */
        fun fromChatBotId(botId: ChatExtensionId): ChatId = forExtension(botId)

        fun extensionPrefixBytes(extensionId: ChatExtensionId): ByteArray {
            return "$EXTENSION_PREFIX$extensionId".encodeToByteArray()
        }

        private const val EXTENSION_PREFIX = "ChatExtension:"
        private val EXTENSION_PREFIX_BYTES = EXTENSION_PREFIX.encodeToByteArray()
    }

    fun chatVariant(): ChatVariant {
        val id = value.value

        return when {
            id.startsWith(EXTENSION_PREFIX_BYTES) -> {
                val decoded = id.decodeToString().removePrefix(EXTENSION_PREFIX)
                val colonIndex = decoded.indexOf(':')
                if (colonIndex == -1) {
                    ChatVariant.Extension(extensionId = decoded, subRoomId = null)
                } else {
                    ChatVariant.Extension(
                        extensionId = decoded.substring(0, colonIndex),
                        subRoomId = decoded.substring(colonIndex + 1)
                    )
                }
            }

            else -> ChatVariant.Contact(value)
        }
    }

    fun isExtensionChat(extensionId: ChatExtensionId): Boolean {
        val variant = chatVariant()
        return variant is ChatVariant.Extension && variant.extensionId == extensionId
    }

    fun isContactChat(contactId: AccountId): Boolean {
        return contactId == value
    }
}

fun ChatId.contactOrThrow(): ChatVariant.Contact {
    return chatVariant() as ChatVariant.Contact
}

fun ChatId.extensionOrFailure(): Result<ChatVariant.Extension> {
    val chatVariant = chatVariant()
    return if (chatVariant is ChatVariant.Extension) {
        Result.success(chatVariant)
    } else {
        Result.failure(IllegalArgumentException("Chat id $value is not an extension chat"))
    }
}

fun ChatId.extensionOrNull(): ChatVariant.Extension? {
    return chatVariant() as? ChatVariant.Extension
}

fun ChatId.contactOrNull(): ChatVariant.Contact? {
    return chatVariant() as? ChatVariant.Contact
}

fun ChatId.isContactChat(): Boolean {
    return chatVariant() is ChatVariant.Contact
}

fun ChatId.isExtensionChat(): Boolean {
    return chatVariant() is ChatVariant.Extension
}

inline fun ChatId.onContact(action: (contactId: ChatVariant.Contact) -> Unit) {
    val contact = contactOrNull() ?: return
    action(contact)
}
