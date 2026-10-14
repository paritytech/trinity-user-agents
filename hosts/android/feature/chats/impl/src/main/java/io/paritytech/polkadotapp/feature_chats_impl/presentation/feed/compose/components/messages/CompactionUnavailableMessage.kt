package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageGrouping
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageSurfaceStyle
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components.FlatMessageTimestamp
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components.TextMessageLayout
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.ChatMessageTimeFormatter
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.LocalChatMessageTimeFormatter
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun CompactionUnavailableMessage(
    modifier: Modifier = Modifier,
    message: ChatMessageUiModel.CompactionUnavailable,
    showTimestamp: Boolean,
    grouping: ChatMessageGrouping,
    isHighlighted: Boolean,
    customBubbleStyle: ChatMessageSurfaceStyle? = null,
) {
    ChatMessageContainer(
        modifier = modifier,
        message = message,
        grouping = grouping,
        isHighlighted = isHighlighted,
        canBeReplied = false,
        onMessageAction = {},
        surfaceStyle = customBubbleStyle ?: ChatMessageSurfaceStyle.default(message.direction),
    ) {
        Content(showTimestamp, message)
    }
}

@Composable
private fun Content(
    showTimestamp: Boolean,
    message: ChatMessageUiModel.CompactionUnavailable
) {
    val timestampComposable = if (showTimestamp) {
        @Composable {
            FlatMessageTimestamp(message = message)
        }
    } else {
        null
    }

    TextMessageLayout(
        text = stringResource(RCommon.string.chat_message_compaction_unavailable),
        style = PolkadotTheme.typography.body.large,
        color = message.direction.defaultTextColor,
        timestamp = timestampComposable,
        direction = message.direction
    )
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun CompactionUnavailableMessagePreview() {
    PolkadotTheme {
        CompositionLocalProvider(
            LocalChatMessageTimeFormatter provides ChatMessageTimeFormatter.mocked()
        ) {
            CompactionUnavailableMessage(
                modifier = Modifier.fillMaxWidth(),
                message = ChatMessageUiModel.CompactionUnavailable(
                    id = "1",
                    direction = ChatMessageUiModel.Direction.INCOMING,
                    status = ChatMessageUiModel.Status.SENT,
                    timestamp = System.currentTimeMillis(),
                    origin = ChatMessageOrigin.User
                ),
                showTimestamp = true,
                grouping = ChatMessageGrouping.Standalone,
                isHighlighted = false
            )
        }
    }
}
