package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.common.R
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageGrouping
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageSurfaceStyle
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components.FlatMessageTimestamp
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components.TextMessageLayout
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.ChatMessageTimeFormatter
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.LocalChatMessageTimeFormatter

@Composable
fun UnsupportedMessage(
    modifier: Modifier = Modifier,
    message: ChatMessageUiModel.Unsupported,
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
    message: ChatMessageUiModel.Unsupported
) {
    val timestampComposable = if (showTimestamp) {
        @Composable {
            FlatMessageTimestamp(message = message)
        }
    } else {
        null
    }

    TextMessageLayout(
        text = stringResource(R.string.chat_message_unsupported),
        style = PolkadotTheme.typography.body.large,
        color = message.direction.defaultTextColor,
        timestamp = timestampComposable,
        direction = message.direction
    )
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun UnsupportedMessagePreview() {
    PolkadotTheme {
        CompositionLocalProvider(
            LocalChatMessageTimeFormatter provides ChatMessageTimeFormatter.mocked()
        ) {
            Column(
                verticalArrangement = Arrangement.spacedBy(16.dp)
            ) {
                UnsupportedMessage(
                    modifier = Modifier.fillMaxWidth(),
                    message = ChatMessageUiModel.Unsupported(
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

                UnsupportedMessage(
                    message = ChatMessageUiModel.Unsupported(
                        id = "2",
                        direction = ChatMessageUiModel.Direction.OUTGOING,
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
}
