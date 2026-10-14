package io.paritytech.polkadotapp.feature_chats_api.domain.model

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_account_api.domain.model.SharedSecretDerivationDomain
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.Username

sealed class OpenChatRequest {
    class ExistingChat(val chatId: ChatId) : OpenChatRequest()

    class StartChatWithContact(
        val contactAccountId: AccountId,
        val username: Username?,
        val avatar: String?,
        val chatKey: ByteArray,
        val sharedSecretDerivationDomain: SharedSecretDerivationDomain,
        val ourMetaAccountId: Long,
        val origin: ContactOrigin,
    ) : OpenChatRequest()
}

fun OpenChatRequest.computeChatId(): ChatId {
    return when (this) {
        is OpenChatRequest.ExistingChat -> chatId
        is OpenChatRequest.StartChatWithContact -> ChatId.fromContact(contactAccountId)
    }
}
