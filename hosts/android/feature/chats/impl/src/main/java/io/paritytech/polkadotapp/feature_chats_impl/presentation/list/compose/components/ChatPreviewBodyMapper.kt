package io.paritytech.polkadotapp.feature_chats_impl.presentation.list.compose.components

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import io.paritytech.polkadotapp.common.R
import io.paritytech.polkadotapp.common.presentation.compose.withCurrencyTickerStyle
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.CallOutlined
import io.paritytech.polkadotapp.design.components.icon.vectors.FileOutlined
import io.paritytech.polkadotapp.design.components.icon.vectors.PhotoSolid
import io.paritytech.polkadotapp.design.components.icon.vectors.VideoOutlined
import io.paritytech.polkadotapp.design.components.text.ensureAnnotatedString
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.design.utils.withBold
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.asAnyRenderer
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatPreviewUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.CustomChatPreviewUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.DraftPreviewUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.LastMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageAttachmentType
import io.paritytech.polkadotapp.feature_chats_impl.presentation.util.formatCallDuration
import io.paritytech.polkadotapp.feature_chats_impl.utils.nameRes
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.LocalTokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.RoundPrecision

data class ChatPreviewBody(
    val text: AnnotatedString,
    val icon: ImageVector? = null,
)

@Composable
internal fun ChatPreviewUiModel.toPreviewBody(username: String): ChatPreviewBody {
    return when (this) {
        is CustomChatPreviewUiModel<*> ->
            renderer.formatChatPreview(data).coverUnsupportedMessage()
                .withCurrencyTickerStyle(PolkadotTheme.typography.paragraph.medium).asPlainBody()

        is DraftPreviewUiModel -> draftPreviewBody(text)

        is LastMessageUiModel -> toPreviewBody(username)
    }
}

@Composable
private fun draftPreviewBody(text: String): ChatPreviewBody {
    val full = stringResource(R.string.chat_last_message_draft_prefix, text)
    val body = remember(full, text) {
        buildAnnotatedString {
            append(full)
            val labelEnd = full.indexOf(text).coerceAtLeast(0)
            addStyle(
                style = SpanStyle(fontWeight = FontWeight.Bold),
                start = 0,
                end = labelEnd,
            )
        }
    }
    return ChatPreviewBody(text = body)
}

@Composable
private fun LastMessageUiModel.toPreviewBody(username: String): ChatPreviewBody {
    return when (this) {
        is LastMessageUiModel.Attachments -> attachmentsPreviewBody()

        is LastMessageUiModel.Call -> callPreviewBody()

        is LastMessageUiModel.ContactAdded -> stringResource(
            if (isIncoming) R.string.chat_message_contact_added_incoming else R.string.chat_message_contact_added_outgoing,
            username
        ).asPlainBody()

        is LastMessageUiModel.Text -> (
            if (isIncoming) message else stringResource(R.string.chat_message_last_outgoing, message)
            ).asPlainBody()

        is LastMessageUiModel.Token -> stringResource(
            if (isIncoming) R.string.chat_last_message_token_incoming else R.string.chat_last_message_token_outgoing,
            username
        ).asPlainBody()

        is LastMessageUiModel.Payment -> {
            val value = LocalTokenAmountFormatter.current.formatTokenAmount(
                tokenAmount = tokenAmount,
                precision = RoundPrecision.DEFAULT
            )
            val text = if (isIncoming) {
                val incomingRes = if (paymentStatus.isClaimInFlight) {
                    R.string.chat_last_message_payment_incoming_sending
                } else {
                    R.string.chat_last_message_payment_incoming
                }
                stringResource(incomingRes, username, value)
            } else {
                stringResource(R.string.chat_last_message_payment_outgoing, value)
            }.withBold(value).withCurrencyTickerStyle(PolkadotTheme.typography.paragraph.medium)
            text.asPlainBody()
        }

        is LastMessageUiModel.Reacted -> (
            if (isIncoming) stringResource(R.string.chat_last_message_reacted_incoming, username, emoji)
            else stringResource(R.string.chat_last_message_reacted_outgoing, emoji)
            ).asPlainBody()

        is LastMessageUiModel.RemovedReaction -> (
            if (isIncoming) stringResource(R.string.chat_last_message_reaction_removed_incoming, username)
            else stringResource(R.string.chat_last_message_reaction_removed_outgoing)
            ).asPlainBody()

        is LastMessageUiModel.Unsupported -> stringResource(R.string.chat_message_unsupported).asPlainBody()

        is LastMessageUiModel.CompactionUnavailable -> stringResource(R.string.chat_message_compaction_unavailable).asPlainBody()

        is LastMessageUiModel.LeftChat -> (
            if (isIncoming) stringResource(R.string.chat_last_message_left_chat_incoming, username).withBold(username)
            else stringResource(R.string.chat_last_message_left_chat_outgoing)
            ).asPlainBody()

        is LastMessageUiModel.Custom<*> -> {
            @Suppress("UNCHECKED_CAST")
            renderer.asAnyRenderer()
                .formatChatPreview(this as LastMessageUiModel.Custom<Any?>)
                .coverUnsupportedMessage()
                .withCurrencyTickerStyle(PolkadotTheme.typography.paragraph.medium)
                .asPlainBody()
        }

        is LastMessageUiModel.ChatAccepted -> (
            if (isIncoming) stringResource(R.string.chat_request_approved, username)
            else stringResource(R.string.chat_request_approved_outgoing)
            ).asPlainBody()
    }
}

@Composable
private fun LastMessageUiModel.Attachments.attachmentsPreviewBody(): ChatPreviewBody {
    val labelText = when {
        !message.isNullOrEmpty() -> message
        count > 1 -> pluralStringResource(type.collectionPluralRes(), count, count)
        else -> stringResource(type.nameRes())
    }
    return ChatPreviewBody(text = AnnotatedString(labelText.orEmpty()), icon = type.icon())
}

@Composable
private fun LastMessageUiModel.Call.callPreviewBody(): ChatPreviewBody {
    val isVideo = purpose == ChatMessageUiModel.Call.Purpose.VIDEO_CALL
    val isOutgoing = !isIncoming
    val label = when (val s = state) {
        ChatMessageUiModel.Call.State.Ringing -> stringResource(
            when {
                isOutgoing && isVideo -> R.string.chat_message_call_outgoing_video
                isOutgoing -> R.string.chat_message_call_outgoing_voice
                isVideo -> R.string.chat_message_call_incoming_video
                else -> R.string.chat_message_call_incoming_voice
            }
        )

        ChatMessageUiModel.Call.State.Ongoing -> stringResource(
            if (isVideo) R.string.chat_message_call_ongoing_video else R.string.chat_message_call_ongoing_voice
        )

        is ChatMessageUiModel.Call.State.Ended -> {
            val base = stringResource(if (isVideo) R.string.chat_message_call_ended_video else R.string.chat_message_call_ended_voice)
            "$base (${formatCallDuration(s.duration)})"
        }

        ChatMessageUiModel.Call.State.Missed -> stringResource(
            if (isVideo) R.string.chat_message_call_missed_video else R.string.chat_message_call_missed_voice
        )

        is ChatMessageUiModel.Call.State.Canceled -> stringResource(
            if (isVideo) R.string.chat_message_call_canceled_video else R.string.chat_message_call_canceled_voice
        )

        is ChatMessageUiModel.Call.State.Declined -> stringResource(
            if (isVideo) R.string.chat_message_call_declined_video else R.string.chat_message_call_declined_voice
        )
    }

    return ChatPreviewBody(
        text = AnnotatedString(label),
        icon = if (isVideo) NovaIcons.VideoOutlined else NovaIcons.CallOutlined,
    )
}

private fun MessageAttachmentType.icon(): ImageVector = when (this) {
    MessageAttachmentType.IMAGE -> NovaIcons.PhotoSolid
    MessageAttachmentType.VIDEO -> NovaIcons.VideoOutlined
    MessageAttachmentType.FILE -> NovaIcons.FileOutlined
}

private fun MessageAttachmentType.collectionPluralRes(): Int = when (this) {
    MessageAttachmentType.IMAGE -> R.plurals.chat_last_message_media_collection_photos
    MessageAttachmentType.VIDEO -> R.plurals.chat_last_message_media_collection_videos
    MessageAttachmentType.FILE -> R.plurals.chat_last_message_media_collection_files
}

private fun CharSequence.asPlainBody(): ChatPreviewBody = ChatPreviewBody(text = ensureAnnotatedString())

@Composable
private fun Result<String>.coverUnsupportedMessage() =
    logFailure("Failed to format custom message")
        .getOrElse { stringResource(R.string.chat_message_unsupported) }
