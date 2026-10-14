package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.common.presentation.formatters.time.LocalTimeFormatter
import io.paritytech.polkadotapp.common.presentation.formatters.time.TimeFormatter
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.CallOutlined
import io.paritytech.polkadotapp.design.components.icon.vectors.VideoOutlined
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageGrouping
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageSurfaceStyle
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageAction
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.icons.CallStatusNegative
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.icons.CallStatusPositive
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components.FlatMessageTimestamp
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.ChatMessageTimeFormatter
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.LocalChatMessageTimeFormatter
import io.paritytech.polkadotapp.feature_chats_impl.presentation.util.formatCallDuration
import kotlin.time.Duration.Companion.minutes
import kotlin.time.Duration.Companion.seconds
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun CallMessage(
    modifier: Modifier = Modifier,
    message: ChatMessageUiModel.Call,
    grouping: ChatMessageGrouping,
    isHighlighted: Boolean,
    onMessageAction: (MessageAction) -> Unit,
    customBubbleStyle: ChatMessageSurfaceStyle? = null,
) {
    ChatMessageContainer(
        modifier = modifier,
        message = message,
        grouping = grouping,
        isHighlighted = isHighlighted,
        canBeReplied = false,
        onMessageAction = onMessageAction,
        surfaceStyle = customBubbleStyle ?: ChatMessageSurfaceStyle.default(message.direction),
    ) {
        Row(
            modifier = Modifier
                .clickable(
                    onClick = { onMessageAction(MessageAction.Press(message)) }
                )
                .padding(PolkadotTheme.spacings.extraMedium),
            horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.extraMedium),
            verticalAlignment = Alignment.CenterVertically
        ) {
            CallAvatar(
                purpose = message.purpose,
                direction = message.direction
            )

            Column {
                NovaText(
                    text = stringResource(message.titleRes()),
                    style = PolkadotTheme.typography.body.large,
                    color = message.direction.defaultTextColor
                )

                Row(
                    verticalAlignment = Alignment.CenterVertically
                ) {
                    NovaIcon(
                        modifier = Modifier.size(16.dp),
                        imageVector = message.state.statusIcon(),
                        tint = message.state.stateColor()
                    )

                    HorizontalSpacer { extraTiny }

                    NovaText(
                        text = message.subtitle(),
                        style = PolkadotTheme.typography.body.smallEmphasized,
                        color = message.direction.subtitleTextColor
                    )
                }
            }

            FlatMessageTimestamp(
                modifier = Modifier.align(Alignment.Bottom),
                message = message
            )
        }
    }
}

@Composable
private fun CallAvatar(
    purpose: ChatMessageUiModel.Call.Purpose,
    direction: ChatMessageUiModel.Direction
) {
    val icon = when (purpose) {
        ChatMessageUiModel.Call.Purpose.AUDIO_CALL -> NovaIcons.CallOutlined
        ChatMessageUiModel.Call.Purpose.VIDEO_CALL -> NovaIcons.VideoOutlined
    }

    val iconColor: Color
    val backgroundColor: Color

    when (direction) {
        ChatMessageUiModel.Direction.INCOMING -> {
            iconColor = PolkadotTheme.colors.fg.primary
            backgroundColor = PolkadotTheme.colors.bg.surface.nested
        }
        ChatMessageUiModel.Direction.OUTGOING -> {
            iconColor = PolkadotTheme.colors.fg.primaryInverted
            backgroundColor = PolkadotTheme.colors.bg.surface.nestedInverted
        }
    }

    PolkadotSurface(
        shape = PolkadotTheme.shapes.full,
        color = backgroundColor,
        contentAlignment = Alignment.Center
    ) {
        NovaIcon(
            modifier = Modifier
                .padding(PolkadotTheme.spacings.small)
                .size(24.dp),
            imageVector = icon,
            tint = iconColor
        )
    }
}

@Composable
private fun ChatMessageUiModel.Call.State.stateColor(): Color {
    return when (this) {
        is ChatMessageUiModel.Call.State.Ringing,
        is ChatMessageUiModel.Call.State.Ongoing,
        is ChatMessageUiModel.Call.State.Ended -> PolkadotTheme.colors.fg.success
        is ChatMessageUiModel.Call.State.Missed,
        is ChatMessageUiModel.Call.State.Canceled,
        is ChatMessageUiModel.Call.State.Declined -> PolkadotTheme.colors.fg.error
    }
}

private fun ChatMessageUiModel.Call.State.statusIcon(): ImageVector {
    return when (this) {
        is ChatMessageUiModel.Call.State.Ringing,
        is ChatMessageUiModel.Call.State.Ongoing,
        is ChatMessageUiModel.Call.State.Ended -> CallStatusPositive
        is ChatMessageUiModel.Call.State.Missed,
        is ChatMessageUiModel.Call.State.Canceled,
        is ChatMessageUiModel.Call.State.Declined -> CallStatusNegative
    }
}

private fun ChatMessageUiModel.Call.titleRes(): Int {
    val isVideo = purpose == ChatMessageUiModel.Call.Purpose.VIDEO_CALL
    val isOutgoing = direction == ChatMessageUiModel.Direction.OUTGOING

    return when (state) {
        is ChatMessageUiModel.Call.State.Ringing -> when {
            isOutgoing && isVideo -> RCommon.string.chat_message_call_outgoing_video
            isOutgoing -> RCommon.string.chat_message_call_outgoing_voice
            isVideo -> RCommon.string.chat_message_call_incoming_video
            else -> RCommon.string.chat_message_call_incoming_voice
        }
        is ChatMessageUiModel.Call.State.Ongoing -> {
            if (isVideo) RCommon.string.chat_message_call_ongoing_video else RCommon.string.chat_message_call_ongoing_voice
        }
        is ChatMessageUiModel.Call.State.Ended -> {
            if (isVideo) RCommon.string.chat_message_call_ended_video else RCommon.string.chat_message_call_ended_voice
        }
        is ChatMessageUiModel.Call.State.Missed -> {
            if (isVideo) RCommon.string.chat_message_call_missed_video else RCommon.string.chat_message_call_missed_voice
        }
        is ChatMessageUiModel.Call.State.Canceled -> RCommon.string.chat_message_call_canceled
        is ChatMessageUiModel.Call.State.Declined -> RCommon.string.chat_message_call_declined
    }
}

@Composable
private fun ChatMessageUiModel.Call.subtitle(): String {
    return when (val state = state) {
        is ChatMessageUiModel.Call.State.Ringing -> stringResource(RCommon.string.chat_message_call_subtitle_calling)
        is ChatMessageUiModel.Call.State.Ongoing -> stringResource(RCommon.string.chat_message_call_subtitle_tap_to_return)
        is ChatMessageUiModel.Call.State.Missed -> stringResource(RCommon.string.chat_message_call_subtitle_tap_to_call_back)
        is ChatMessageUiModel.Call.State.Ended -> formatCallDuration(state.duration)
        is ChatMessageUiModel.Call.State.Canceled -> stringResource(RCommon.string.chat_message_call_subtitle_tap_to_call_again)
        is ChatMessageUiModel.Call.State.Declined -> when (direction) {
            ChatMessageUiModel.Direction.INCOMING -> stringResource(RCommon.string.chat_message_call_subtitle_tap_to_call_back)
            ChatMessageUiModel.Direction.OUTGOING -> stringResource(RCommon.string.chat_message_call_subtitle_tap_to_call_again)
        }
    }
}

@Preview(showBackground = true, backgroundColor = 0xFF000000, device = "spec:width=1080px,height=3000px,dpi=440")
@Composable
private fun CallMessagePreview() {
    PolkadotTheme {
        CompositionLocalProvider(
            LocalChatMessageTimeFormatter provides ChatMessageTimeFormatter.mocked(),
            LocalTimeFormatter provides TimeFormatter.mocked(LocalContext.current)
        ) {
            Column(
                modifier = Modifier.fillMaxWidth(),
                verticalArrangement = Arrangement.spacedBy(8.dp)
            ) {
                listOf(
                    ChatMessageUiModel.Call.State.Ringing,
                    ChatMessageUiModel.Call.State.Ongoing,
                    ChatMessageUiModel.Call.State.Ended(3.minutes),
                    ChatMessageUiModel.Call.State.Missed,
                    ChatMessageUiModel.Call.State.Canceled(15.seconds),
                    ChatMessageUiModel.Call.State.Declined(20.seconds)
                ).forEach { state ->
                    CallMessage(
                        modifier = Modifier.fillMaxWidth(),
                        message = ChatMessageUiModel.Call(
                            id = "1",
                            timestamp = System.currentTimeMillis(),
                            direction = ChatMessageUiModel.Direction.INCOMING,
                            status = ChatMessageUiModel.Status.READ,
                            origin = ChatMessageOrigin.User,
                            purpose = ChatMessageUiModel.Call.Purpose.VIDEO_CALL,
                            state = state
                        ),
                        grouping = ChatMessageGrouping.Standalone,
                        isHighlighted = false,
                        onMessageAction = {}
                    )

                    CallMessage(
                        modifier = Modifier.fillMaxWidth(),
                        message = ChatMessageUiModel.Call(
                            id = "2",
                            timestamp = System.currentTimeMillis(),
                            direction = ChatMessageUiModel.Direction.OUTGOING,
                            status = ChatMessageUiModel.Status.READ,
                            origin = ChatMessageOrigin.User,
                            purpose = ChatMessageUiModel.Call.Purpose.AUDIO_CALL,
                            state = state
                        ),
                        grouping = ChatMessageGrouping.Standalone,
                        isHighlighted = false,
                        onMessageAction = {}
                    )
                }
            }
        }
    }
}
