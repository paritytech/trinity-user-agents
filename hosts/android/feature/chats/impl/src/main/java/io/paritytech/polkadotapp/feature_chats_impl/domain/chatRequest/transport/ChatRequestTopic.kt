package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.transport

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey

sealed class ChatRequestTopic {
    abstract val acceptor: AccountId

    data class Full(override val acceptor: AccountId) : ChatRequestTopic()

    data class Day(override val acceptor: AccountId, val day: Long) : ChatRequestTopic()

    data class Session(
        val peerAccountId: AccountId,
        val peerChatKey: X25519PublicKey,
        val pin: String?,
        val ourAccountId: AccountId,
        val direction: Direction
    ) : ChatRequestTopic() {
        override val acceptor: AccountId
            get() = when (direction) {
                Direction.FROM_PEER -> ourAccountId
                Direction.TO_PEER -> peerAccountId
            }

        val requester: AccountId
            get() = when (direction) {
                Direction.FROM_PEER -> peerAccountId
                Direction.TO_PEER -> ourAccountId
            }

        enum class Direction {
            FROM_PEER, TO_PEER
        }
    }
}
