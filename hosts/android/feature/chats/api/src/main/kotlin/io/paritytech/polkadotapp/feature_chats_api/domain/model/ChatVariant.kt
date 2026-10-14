package io.paritytech.polkadotapp.feature_chats_api.domain.model

import io.paritytech.polkadotapp.common.domain.model.AccountId

typealias ChatExtensionId = String

sealed interface ChatVariant {
    data class Extension(val extensionId: ChatExtensionId, val subRoomId: String?) : ChatVariant

    data class Contact(val contactAccountId: AccountId) : ChatVariant
}

fun ChatVariant.contactOrNull(): ChatVariant.Contact? {
    return this as ChatVariant.Contact
}
