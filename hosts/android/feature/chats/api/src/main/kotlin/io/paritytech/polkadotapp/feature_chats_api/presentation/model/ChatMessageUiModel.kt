package io.paritytech.polkadotapp.feature_chats_api.presentation.model

import android.content.ContentResolver
import android.net.Uri
import androidx.compose.runtime.Immutable
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.unit.IntSize
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import io.paritytech.polkadotapp.common.utils.InformationSize
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.CustomChatMessageRenderer
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.ImmutableSet
import kotlin.time.Duration

@Immutable
sealed interface ChatMessageUiModel {
    val id: ChatMessageId
    val timestamp: Timestamp
    val direction: Direction
    val status: Status
    val origin: ChatMessageOrigin

    data class Text(
        override val id: ChatMessageId,
        override val timestamp: Timestamp,
        override val direction: Direction,
        override val status: Status,
        override val origin: ChatMessageOrigin,
        val text: String,
        val replyPreview: ReplyPreview?,
        val reactions: ImmutableList<Reaction>,
        val isEdited: Boolean
    ) : ChatMessageUiModel

    data class ContactAdded(
        override val id: ChatMessageId,
        override val timestamp: Timestamp,
        override val direction: Direction,
        override val status: Status,
        override val origin: ChatMessageOrigin,
    ) : ChatMessageUiModel

    data class CoinagePayment(
        override val id: ChatMessageId,
        override val timestamp: Timestamp,
        override val direction: Direction,
        override val status: ChatMessageUiModel.Status,
        override val origin: ChatMessageOrigin,
        val paymentStatus: Status,
        val amount: TokenAmountModel,
        val reactions: ImmutableList<Reaction>
    ) : ChatMessageUiModel {
        /**
         * Only a finished claim can be short. While claiming is still going the amount seen so far says
         * nothing about the final one, and showing a shortfall then tells the user they were short-changed
         * by a payment that may yet arrive in full.
         */
        val differingAmount: TokenAmountModel?
            get() = (paymentStatus as? Status.Transferred)?.transferred
                ?.takeIf { it.amount.compareTo(amount.amount) != 0 }

        @Immutable
        sealed interface Status {
            /**
             * Claiming has not finished, so the funds have not arrived yet. A partial claim counts as in
             * flight: the rest may still land.
             */
            val isClaimInFlight: Boolean
                get() = when (this) {
                    is Detecting,
                    is Detected,
                    is PartiallyClaimed -> true

                    is Transferred,
                    is FailedDetection,
                    is FailedTransfer -> false
                }

            data object Detecting : Status
            data class Detected(val detected: TokenAmountModel) : Status
            data class PartiallyClaimed(val claimed: TokenAmountModel) : Status
            data class Transferred(val transferred: TokenAmountModel) : Status
            data object FailedDetection : Status
            data object FailedTransfer : Status
        }
    }

    data class Multimedia(
        override val id: ChatMessageId,
        override val timestamp: Timestamp,
        override val direction: Direction,
        override val status: Status,
        override val origin: ChatMessageOrigin,
        val uri: Uri?,
        val text: String?,
        val replyPreview: ReplyPreview?,
        val type: MultimediaType,
        val blurHash: String?,
        val reactions: ImmutableList<Reaction>,
        val isEdited: Boolean
    ) : ChatMessageUiModel {
        @Immutable
        sealed interface MultimediaType {
            data class Image(val height: Int, val width: Int) : MultimediaType
            data class Video(val duration: Duration) : MultimediaType
        }

        fun isOpenable(): Boolean = uri != null && uri.scheme != ContentResolver.SCHEME_ANDROID_RESOURCE
    }

    data class File(
        override val id: ChatMessageId,
        override val timestamp: Timestamp,
        override val direction: Direction,
        override val status: Status,
        override val origin: ChatMessageOrigin,
        val uri: Uri,
        val fileName: String,
        val size: InformationSize,
        val thumbnailUri: Uri?,
        val text: String?,
    ) : ChatMessageUiModel

    data class Unsupported(
        override val id: ChatMessageId,
        override val timestamp: Timestamp,
        override val direction: Direction,
        override val status: Status,
        override val origin: ChatMessageOrigin,
    ) : ChatMessageUiModel

    data class CompactionUnavailable(
        override val id: ChatMessageId,
        override val timestamp: Timestamp,
        override val direction: Direction,
        override val status: Status,
        override val origin: ChatMessageOrigin,
    ) : ChatMessageUiModel

    data class ChatRequest(
        override val id: ChatMessageId,
        override val timestamp: Timestamp,
        override val direction: Direction,
        override val status: Status,
        override val origin: ChatMessageOrigin,
        val welcomeText: String?,
        val reactions: ImmutableList<Reaction>,
    ) : ChatMessageUiModel

    data class ChatAccepted(
        override val id: ChatMessageId,
        override val timestamp: Timestamp,
        override val direction: Direction,
        override val status: Status,
        override val origin: ChatMessageOrigin,
        val peerUsername: String
    ) : ChatMessageUiModel

    data class Call(
        override val id: ChatMessageId,
        override val timestamp: Timestamp,
        override val direction: Direction,
        override val status: Status,
        override val origin: ChatMessageOrigin,
        val purpose: Purpose,
        val state: State
    ) : ChatMessageUiModel {
        enum class Purpose {
            AUDIO_CALL, VIDEO_CALL
        }

        sealed interface State {
            data object Ringing : State
            data object Ongoing : State
            data class Ended(val duration: Duration) : State
            data object Missed : State
            data class Canceled(val duration: Duration) : State
            data class Declined(val duration: Duration) : State
        }
    }

    data class Custom<T>(
        override val id: String,
        override val timestamp: Timestamp,
        override val direction: Direction,
        override val status: Status,
        override val origin: ChatMessageOrigin,
        val renderer: CustomChatMessageRenderer<T>,
        val content: Result<T>,
        val chatId: ChatId,
    ) : ChatMessageUiModel

    enum class Status { PENDING, SENT, READ, FAILED }
    enum class Direction { INCOMING, OUTGOING }

    data class Reaction(val count: Int, val emoji: String, val reactedByUser: Boolean)
}

@Immutable
data class ReplyPreview(
    val messageId: ChatMessageId,
    val title: String,
    val content: Content
) {
    sealed interface Content {
        data class Text(val text: String) : Content
        data class Image(val thumbnailUri: Uri?, val caption: String?) : Content
        data class Video(val thumbnailUri: Uri?, val caption: String?) : Content
        data class File(val fileName: String, val caption: String?) : Content
        data class Payment(val amount: TokenAmountModel, val direction: ChatMessageUiModel.Direction) : Content
    }
}

@Immutable
data class HighlightedMessage(
    val messageId: ChatMessageId,
    val scrollIndex: Int
)

@Immutable
data class FirstNewMessageInfo(
    val index: Int,
    val messageId: ChatMessageId
)

@Immutable
data class MessageRevisionUiModel(
    val text: String,
    val timestamp: Long
)

sealed class MessagePopUpUiState {
    data class ReactionsDetails(
        val messageId: ChatMessageId,
        val reactionsByEmoji: ImmutableList<EmojiReactionGroup>,
        val totalReactionsCount: Int
    ) : MessagePopUpUiState()

    data class ActionMenu(
        val message: ChatMessageUiModel,
        val userReactedEmojis: ImmutableSet<String>,
        val canLeaveReactions: Boolean,
        val allowedMenuActions: ImmutableList<AllowedMessageMenuAction>
    ) : MessagePopUpUiState()
}

fun ChatMessageUiModel.isUnread(): Boolean {
    return direction == ChatMessageUiModel.Direction.INCOMING && status != ChatMessageUiModel.Status.READ
}

fun ChatMessageUiModel.isOutgoing(): Boolean {
    return direction == ChatMessageUiModel.Direction.OUTGOING
}

fun ChatMessageUiModel.isIncoming(): Boolean {
    return direction == ChatMessageUiModel.Direction.INCOMING
}

data class MessageLayoutInfo(
    val offset: Offset,
    val size: IntSize
)

sealed interface MessageAction {
    data class LongPress(val message: ChatMessageUiModel) : MessageAction
    data class Press(val message: ChatMessageUiModel) : MessageAction
    data class Reply(val message: ChatMessageUiModel) : MessageAction
    data class Copy(val text: String) : MessageAction
    data class Edit(val message: ChatMessageUiModel, val text: String) : MessageAction
    data class Reaction(val message: ChatMessageUiModel, val emoji: String) : MessageAction
    data class ShowReactionDetails(val message: ChatMessageUiModel) : MessageAction
    data class ReplyPreviewTap(val messageId: ChatMessageId) : MessageAction
    data class ViewEditHistory(val message: ChatMessageUiModel) : MessageAction
    data object DismissActionMenu : MessageAction
}

sealed interface AllowedMessageMenuAction {
    data object Reply : AllowedMessageMenuAction
    data class Copy(val text: String) : AllowedMessageMenuAction
    data class Edit(val text: String) : AllowedMessageMenuAction
    data object ViewEditHistory : AllowedMessageMenuAction
}
