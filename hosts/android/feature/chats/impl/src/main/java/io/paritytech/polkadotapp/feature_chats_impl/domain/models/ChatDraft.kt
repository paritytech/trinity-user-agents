package io.paritytech.polkadotapp.feature_chats_impl.domain.models

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId

data class ChatDraft(
    val text: String,
    val relation: ChatDraftRelation,
)

sealed interface ChatDraftRelation {
    data object None : ChatDraftRelation

    data class Reply(val messageId: ChatMessageId) : ChatDraftRelation

    data class Edit(
        val messageId: ChatMessageId,
        val originalText: String,
    ) : ChatDraftRelation
}

fun ChatDraft.isEmpty(): Boolean = text.isBlank() && relation is ChatDraftRelation.None
