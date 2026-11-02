package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.acceptance

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequest
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatRequestId
import kotlin.time.Instant

/**
 * Our `DeviceChatAccepted` for [requestId], published on the identity route and re-published every period until the
 * contact shows up on its device route (chat-spec RFC-0005 §2). [messageId] is the chat message that carries it, so
 * every renewal re-publishes the same message.
 */
class ChatAcceptance(
    val contactAccountId: AccountId,
    val requestId: ChatRequestId,
    val messageId: ChatMessageId,
    val acceptedAt: Instant,
    val delivery: ChatRequest.Delivery,
)
