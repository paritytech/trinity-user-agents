package io.paritytech.polkadotapp.feature_chats_api.domain.model

import io.paritytech.polkadotapp.common.domain.model.AccountId

sealed interface ChatMessageOrigin {
    companion object {
        fun fromContactChat(authorAccountId: AccountId, contactAccountId: AccountId): ChatMessageOrigin {
            return if (authorAccountId == contactAccountId) {
                Contact(contactAccountId)
            } else {
                User
            }
        }
    }

    object User : ChatMessageOrigin

    class Contact(val contactAccountId: AccountId) : ChatMessageOrigin

    class Extension(val extensionId: ChatExtensionId) : ChatMessageOrigin
}

fun ChatMessageOrigin.isUser(): Boolean {
    return this is ChatMessageOrigin.User
}
