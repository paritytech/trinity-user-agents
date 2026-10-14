package io.paritytech.polkadotapp.feature_chats_api.domain

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import java.util.UUID

interface ChatMessageSender {
    /**
     * [onSaved] runs inside the transaction that persists the message, so a caller with a fact that must
     * become durable exactly when the message does — a payment's keys are now on their way — can commit it
     * there and nowhere else.
     */
    suspend fun sendUserMessage(
        messageId: ChatMessageId = UUID.randomUUID().toString(),
        chatId: ChatId,
        content: ChatMessage.Content,
        replyToMessageId: String? = null,
        onSaved: suspend () -> Unit = {},
    ): ChatMessage

    context(scope: ComputationalScope)
    fun startExtensions()
}
