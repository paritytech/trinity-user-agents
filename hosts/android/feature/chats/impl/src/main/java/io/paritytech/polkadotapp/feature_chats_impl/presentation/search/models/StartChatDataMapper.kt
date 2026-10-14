package io.paritytech.polkadotapp.feature_chats_impl.presentation.search.models

import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatFeedPayload
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.StartChatData

fun StartChatData.toChatFeedPayload(): ChatFeedPayload = when (this) {
    is StartChatData.NewChat -> ChatFeedPayload.startChatWithContact(
        contactAccountId = contactAccountId,
        username = username,
        avatar = avatar,
        chatKey = chatKey,
        sharedSecretDerivationDomain = sharedSecretDerivationDomain,
        ourMetaAccountId = ourMetaAccountId,
        origin = origin
    )

    is StartChatData.ExistingChat -> ChatFeedPayload.existingContactChat(contactAccountId)
}
