package io.paritytech.polkadotapp.feature_chats_impl.domain.sessions

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatPushId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId

interface ChatSessionCallbacks {
    suspend fun onShouldNotifyNewMessageSent(
        messageId: ChatMessageId,
        accountId: AccountId,
        pushId: ChatPushId,
        encryptedMessage: ByteArray,
        isVoIP: Boolean
    )

    /** The contact sent us something on its device route, so it has processed our acceptance (chat-spec RFC-0005 §2). */
    suspend fun onContactSeenOnDeviceRoute(contactAccountId: AccountId)
}
