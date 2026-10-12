package io.paritytech.polkadotapp.feature_products_api.model

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.extensionOrFailure

@JvmInline
value class ProductChatIdParameter(val value: String)

fun ProductChatIdParameter.toChatId(extensionId: ChatExtensionId): ChatId {
    return ChatId.forExtensionRoom(extensionId, value)
}

fun ChatId.extractProductChatIdParameter(extensionId: ChatExtensionId): Result<ProductChatIdParameter> {
    return extensionOrFailure().mapCatching {
        require(extensionId == it.extensionId) {
            "ChatId $value is not a product chat id"
        }

        val subRoomId = requireNotNull(it.subRoomId) {
            "ChatId $value is not a product chat id"
        }
        require(subRoomId.isNotEmpty()) {
            "ChatId $value carries no product room id"
        }

        ProductChatIdParameter(subRoomId)
    }
}

fun ChatId.productRoomId(extensionId: ChatExtensionId): ProductChatIdParameter? =
    extractProductChatIdParameter(extensionId).getOrNull()
