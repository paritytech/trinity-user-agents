package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
data class MessagePopUpSelection(
    val messageId: ChatMessageId,
    val type: Type
) {
    companion object {
        fun actions(messageId: ChatMessageId): MessagePopUpSelection {
            return MessagePopUpSelection(messageId, Type.ACTIONS)
        }

        fun reactionDetails(messageId: ChatMessageId): MessagePopUpSelection {
            return MessagePopUpSelection(messageId, Type.REACTION_DETAILS)
        }
    }

    enum class Type {
        ACTIONS, REACTION_DETAILS
    }
}
