package io.paritytech.polkadotapp.feature_chats_api.presentation.model

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatMessageRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatPreviewRenderer
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel

@Immutable
sealed interface ChatPreviewUiModel {
    val timestamp: Timestamp
}

data class CustomChatPreviewUiModel<T>(
    override val timestamp: Timestamp,
    val data: T,
    val renderer: CustomChatPreviewRenderer<T>
) : ChatPreviewUiModel

data class DraftPreviewUiModel(
    override val timestamp: Timestamp,
    val text: String
) : ChatPreviewUiModel

enum class MessageAttachmentType {
    IMAGE, VIDEO, FILE
}

@Immutable
sealed interface LastMessageUiModel : ChatPreviewUiModel {
    val isIncoming: Boolean

    data class Text(
        override val timestamp: Timestamp,
        override val isIncoming: Boolean,
        val message: String
    ) : LastMessageUiModel

    data class Attachments(
        override val timestamp: Timestamp,
        override val isIncoming: Boolean,
        val type: MessageAttachmentType,
        val count: Int,
        val message: String?
    ) : LastMessageUiModel

    data class ContactAdded(
        override val timestamp: Timestamp,
        override val isIncoming: Boolean,
    ) : LastMessageUiModel

    data class ChatAccepted(
        override val timestamp: Timestamp,
        override val isIncoming: Boolean,
    ) : LastMessageUiModel

    data class Token(
        override val timestamp: Timestamp,
        override val isIncoming: Boolean,
    ) : LastMessageUiModel

    data class Payment(
        override val timestamp: Timestamp,
        override val isIncoming: Boolean,
        val tokenAmount: TokenAmountModel,
        val paymentStatus: ChatMessageUiModel.CoinagePayment.Status
    ) : LastMessageUiModel

    data class Reacted(
        override val timestamp: Timestamp,
        override val isIncoming: Boolean,
        val emoji: String
    ) : LastMessageUiModel

    data class RemovedReaction(
        override val timestamp: Timestamp,
        override val isIncoming: Boolean,
    ) : LastMessageUiModel

    data class Unsupported(
        override val timestamp: Timestamp,
        override val isIncoming: Boolean,
    ) : LastMessageUiModel

    data class CompactionUnavailable(
        override val timestamp: Timestamp,
        override val isIncoming: Boolean,
    ) : LastMessageUiModel

    data class LeftChat(
        override val timestamp: Timestamp,
        override val isIncoming: Boolean,
    ) : LastMessageUiModel

    data class Call(
        override val timestamp: Timestamp,
        override val isIncoming: Boolean,
        val purpose: ChatMessageUiModel.Call.Purpose,
        val state: ChatMessageUiModel.Call.State
    ) : LastMessageUiModel

    data class Custom<T>(
        override val timestamp: Timestamp,
        override val isIncoming: Boolean,
        val renderer: CustomChatMessageRenderer<T>,
        val content: Result<T>
    ) : LastMessageUiModel
}
