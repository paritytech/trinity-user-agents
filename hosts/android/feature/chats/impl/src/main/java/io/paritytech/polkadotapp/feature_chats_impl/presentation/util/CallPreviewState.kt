package io.paritytech.polkadotapp.feature_chats_impl.presentation.util

import androidx.compose.runtime.Composable
import androidx.compose.ui.res.pluralStringResource
import io.paritytech.polkadotapp.common.R
import io.paritytech.polkadotapp.common.presentation.formatters.time.LocalTimeFormatter
import io.paritytech.polkadotapp.feature_calls_api.domain.models.ActiveCallState
import io.paritytech.polkadotapp.feature_calls_api.domain.models.CallStatus
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessage
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageDirection
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.direction
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.minutes

internal class CallResolutionContext(
    val answersByOfferId: Map<ChatMessageId, ChatMessage>,
    val closedByOfferId: Map<ChatMessageId, ChatMessage>,
    val activeCall: ActiveCallState?
)

internal fun buildCallResolutionContext(
    messages: List<ChatMessage>,
    activeCall: ActiveCallState?
): CallResolutionContext {
    val answers = mutableMapOf<ChatMessageId, ChatMessage>()
    val closed = mutableMapOf<ChatMessageId, ChatMessage>()

    for (msg in messages) {
        when (val content = msg.content) {
            is ChatMessage.Content.DataChannelAnswer -> {
                val existing = answers[content.offerMessageId]
                if (existing == null || msg.timestamp < existing.timestamp) {
                    answers[content.offerMessageId] = msg
                }
            }
            is ChatMessage.Content.DataChannelClosed -> {
                val existing = closed[content.offerMessageId]
                if (existing == null || msg.timestamp < existing.timestamp) {
                    closed[content.offerMessageId] = msg
                }
            }
            else -> Unit
        }
    }

    return CallResolutionContext(
        answersByOfferId = answers,
        closedByOfferId = closed,
        activeCall = activeCall
    )
}

internal fun resolveCallState(
    offer: ChatMessage,
    callContext: CallResolutionContext
): ChatMessageUiModel.Call.State {
    callContext.activeCall.toLiveCallState(offer)?.let { return it }

    val answer = callContext.answersByOfferId[offer.id]
    val closed = callContext.closedByOfferId[offer.id]

    return when {
        closed == null && answer == null -> ChatMessageUiModel.Call.State.Ringing
        closed == null -> ChatMessageUiModel.Call.State.Ongoing
        answer != null -> {
            val duration = (closed.timestamp - answer.timestamp).milliseconds
            ChatMessageUiModel.Call.State.Ended(duration)
        }
        else -> {
            val duration = (closed.timestamp - offer.timestamp).milliseconds
            val offerOutgoing = offer.direction == ChatMessageDirection.OUTGOING
            val closedOutgoing = closed.direction == ChatMessageDirection.OUTGOING

            when {
                offerOutgoing && closedOutgoing -> ChatMessageUiModel.Call.State.Canceled(duration)
                !offerOutgoing && !closedOutgoing -> ChatMessageUiModel.Call.State.Missed
                else -> ChatMessageUiModel.Call.State.Declined(duration)
            }
        }
    }
}

internal fun ActiveCallState?.toLiveCallState(offer: ChatMessage): ChatMessageUiModel.Call.State? {
    if (this == null || offerId != offer.id) return null

    return when (status) {
        CallStatus.Requesting,
        CallStatus.Ringing -> ChatMessageUiModel.Call.State.Ringing

        CallStatus.Connecting,
        is CallStatus.Connected -> ChatMessageUiModel.Call.State.Ongoing

        CallStatus.Ended,
        CallStatus.Failed -> null
    }
}

internal fun ChatMessage.Content.DataChannelOffer.Purpose.toCallPurposeUi(): ChatMessageUiModel.Call.Purpose {
    return when (this) {
        ChatMessage.Content.DataChannelOffer.Purpose.AUDIO_CALL -> ChatMessageUiModel.Call.Purpose.AUDIO_CALL
        ChatMessage.Content.DataChannelOffer.Purpose.VIDEO_CALL -> ChatMessageUiModel.Call.Purpose.VIDEO_CALL
    }
}

@Composable
internal fun formatCallDuration(duration: Duration): String {
    return if (duration < 1.minutes) {
        val seconds = duration.inWholeSeconds.toInt()
        pluralStringResource(R.plurals.common_seconds_format, seconds, seconds)
    } else {
        LocalTimeFormatter.current.formatDuration(duration)
    }
}
