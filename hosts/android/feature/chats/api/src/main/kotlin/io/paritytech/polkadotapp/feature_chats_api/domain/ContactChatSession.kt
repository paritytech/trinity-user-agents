package io.paritytech.polkadotapp.feature_chats_api.domain

import io.paritytech.polkadotapp.common.domain.model.DataByteArray

typealias ChatPushId = DataByteArray
typealias ChatPushToken = DataByteArray

interface ContactChatSession {
    val incomingPushId: ChatPushId
    val outgoingPushId: ChatPushId

    suspend fun sendToken(token: String)
}
