package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages

import androidx.compose.foundation.layout.*
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.common.utils.isSingleEmoji
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageGrouping
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageSurfaceStyle
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageAction
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageLayoutInfo
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ReplyPreview
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components.FilledMessageTimestamp
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components.FlatEditedLabel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components.FlatMessageTimestamp
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components.TextMessageLayout
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components.rememberRevealedCharCount
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.ChatMessageTimeFormatter
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.LocalChatMessageTimeFormatter
import kotlinx.collections.immutable.persistentListOf

@Composable
fun TextMessage(
    modifier: Modifier = Modifier,
    message: ChatMessageUiModel.Text,
    showTimestamp: Boolean,
    grouping: ChatMessageGrouping,
    isHighlighted: Boolean,
    onMessageAction: (MessageAction) -> Unit,
    onLongPress: (MessageLayoutInfo) -> Unit,
    canBeReplied: Boolean,
    customBubbleStyle: ChatMessageSurfaceStyle? = null,
    isRevealing: Boolean = false,
    onRevealComplete: () -> Unit = {},
) {
    val isSingleEmoji = remember(message.text) { message.text.isSingleEmoji() }
    val visibleCharCount = rememberRevealedCharCount(message.text, isRevealing, onRevealComplete)
    val surfaceStyle = when {
        isSingleEmoji -> ChatMessageSurfaceStyle.Transparent
        customBubbleStyle != null -> customBubbleStyle
        else -> ChatMessageSurfaceStyle.default(message.direction)
    }

    ChatMessageContainer(
        modifier = modifier,
        message = message,
        grouping = grouping,
        isHighlighted = isHighlighted,
        canBeReplied = canBeReplied,
        onMessageAction = onMessageAction,
        onLongPress = onLongPress,
        reactions = message.reactions,
        replyPreview = message.replyPreview,
        surfaceStyle = surfaceStyle
    ) {
        if (isSingleEmoji) {
            EmojiOnlyContent(
                message = message,
                showTimestamp = showTimestamp
            )
        } else {
            TextMessageContent(
                showTimestamp = showTimestamp,
                message = message,
                text = message.text,
                isEdited = message.isEdited,
                visibleCharCount = visibleCharCount,
                textColor = surfaceStyle.textColor ?: message.direction.defaultTextColor,
            )
        }
    }
}

@Composable
fun TextMessageContent(
    showTimestamp: Boolean,
    message: ChatMessageUiModel,
    text: String,
    isEdited: Boolean,
    visibleCharCount: Int = Int.MAX_VALUE,
    textColor: Color = message.direction.defaultTextColor,
) {
    val editedLabelComposable = if (isEdited) {
        @Composable {
            FlatEditedLabel(direction = message.direction)
        }
    } else {
        null
    }

    val timestampComposable = if (showTimestamp) {
        @Composable {
            FlatMessageTimestamp(message = message)
        }
    } else {
        null
    }

    TextMessageLayout(
        modifier = Modifier.fillMaxWidth(),
        text = text,
        style = PolkadotTheme.typography.body.large,
        color = textColor,
        direction = message.direction,
        status = editedLabelComposable,
        timestamp = timestampComposable,
        visibleCharCount = visibleCharCount,
    )
}

@Composable
fun EmojiOnlyContent(
    message: ChatMessageUiModel.Text,
    showTimestamp: Boolean
) {
    val timestampAlignment = when (message.direction) {
        ChatMessageUiModel.Direction.INCOMING -> Alignment.BottomStart
        ChatMessageUiModel.Direction.OUTGOING -> Alignment.BottomEnd
    }

    Box {
        NovaText(
            text = message.text,
            style = PolkadotTheme.typography.emoji.large
        )

        Row(
            modifier = Modifier.align(timestampAlignment),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.tiny)
        ) {
            if (message.isEdited) {
                FlatEditedLabel(
                    direction = message.direction
                )
            }
            if (showTimestamp) {
                FilledMessageTimestamp(
                    message = message
                )
            }
        }
    }
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun MessagesPreview() {
    PolkadotTheme {
        CompositionLocalProvider(
            LocalChatMessageTimeFormatter provides ChatMessageTimeFormatter.mocked()
        ) {
            Column(
                modifier = Modifier.fillMaxWidth(),
                verticalArrangement = Arrangement.spacedBy(16.dp)
            ) {
                TextMessage(
                    modifier = Modifier.fillMaxWidth(),
                    message = ChatMessageUiModel.Text(
                        id = "1",
                        text = "Hello!",
                        direction = ChatMessageUiModel.Direction.INCOMING,
                        status = ChatMessageUiModel.Status.SENT,
                        timestamp = System.currentTimeMillis(),
                        reactions = persistentListOf(),
                        origin = ChatMessageOrigin.User,
                        isEdited = false,
                        replyPreview = null
                    ),
                    grouping = ChatMessageGrouping.Standalone,
                    showTimestamp = false,
                    isHighlighted = false,
                    onMessageAction = {},
                    onLongPress = {},
                    canBeReplied = true
                )

                TextMessage(
                    modifier = Modifier.fillMaxWidth(),
                    message = ChatMessageUiModel.Text(
                        id = "2",
                        text = "Hi! How are you doing today? 123",
                        direction = ChatMessageUiModel.Direction.INCOMING,
                        status = ChatMessageUiModel.Status.READ,
                        timestamp = System.currentTimeMillis(),
                        reactions = persistentListOf(
                            ChatMessageUiModel.Reaction(count = 3, emoji = "👍", reactedByUser = true),
                            ChatMessageUiModel.Reaction(count = 1, emoji = "❤️", reactedByUser = false)
                        ),
                        origin = ChatMessageOrigin.User,
                        replyPreview = null,
                        isEdited = false
                    ),
                    grouping = ChatMessageGrouping.Standalone,
                    showTimestamp = true,
                    isHighlighted = false,
                    onMessageAction = {},
                    onLongPress = {},
                    canBeReplied = true
                )

                TextMessage(
                    modifier = Modifier.fillMaxWidth(),
                    message = ChatMessageUiModel.Text(
                        id = "3",
                        text = "Heou?",
                        direction = ChatMessageUiModel.Direction.INCOMING,
                        status = ChatMessageUiModel.Status.SENT,
                        timestamp = System.currentTimeMillis(),
                        reactions = persistentListOf(
                            ChatMessageUiModel.Reaction(count = 1, emoji = "❤️", reactedByUser = false)
                        ),
                        replyPreview = ReplyPreview(
                            messageId = "1",
                            title = "avokado.99",
                            content = ReplyPreview.Content.Text("Hello, want to chat?")
                        ),
                        origin = ChatMessageOrigin.User,
                        isEdited = false
                    ),
                    grouping = ChatMessageGrouping.Standalone,
                    showTimestamp = true,
                    isHighlighted = false,
                    onMessageAction = {},
                    onLongPress = {},
                    canBeReplied = true
                )

                TextMessage(
                    modifier = Modifier.fillMaxWidth(),
                    message = ChatMessageUiModel.Text(
                        id = "4",
                        text = "Hi! How are you?",
                        direction = ChatMessageUiModel.Direction.OUTGOING,
                        status = ChatMessageUiModel.Status.SENT,
                        timestamp = System.currentTimeMillis(),
                        reactions = persistentListOf(
                            ChatMessageUiModel.Reaction(count = 1, emoji = "❤️", reactedByUser = false)
                        ),
                        replyPreview = ReplyPreview(
                            messageId = "2",
                            title = "pineapple.77",
                            content = ReplyPreview.Content.Text(
                                "Hello, want to chat? I have a big question for you... so I need to know if you're available."
                            )
                        ),
                        origin = ChatMessageOrigin.User,
                        isEdited = true
                    ),
                    grouping = ChatMessageGrouping.Standalone,
                    showTimestamp = true,
                    isHighlighted = false,
                    onMessageAction = {},
                    onLongPress = {},
                    canBeReplied = true
                )

                TextMessage(
                    modifier = Modifier.fillMaxWidth(),
                    message = ChatMessageUiModel.Text(
                        id = "5",
                        text = "Hi! How are you?",
                        direction = ChatMessageUiModel.Direction.OUTGOING,
                        status = ChatMessageUiModel.Status.SENT,
                        timestamp = System.currentTimeMillis(),
                        reactions = persistentListOf(
                            ChatMessageUiModel.Reaction(count = 1, emoji = "❤️", reactedByUser = false)
                        ),
                        replyPreview = ReplyPreview(
                            messageId = "2",
                            title = "mrx.12",
                            content = ReplyPreview.Content.Text("Okay")
                        ),
                        origin = ChatMessageOrigin.User,
                        isEdited = true
                    ),
                    grouping = ChatMessageGrouping.Standalone,
                    showTimestamp = true,
                    isHighlighted = false,
                    onMessageAction = {},
                    onLongPress = {},
                    canBeReplied = false
                )

                TextMessage(
                    modifier = Modifier.fillMaxWidth(),
                    message = ChatMessageUiModel.Text(
                        id = "6",
                        text = "❤️",
                        direction = ChatMessageUiModel.Direction.OUTGOING,
                        status = ChatMessageUiModel.Status.SENT,
                        timestamp = System.currentTimeMillis(),
                        reactions = persistentListOf(
                            ChatMessageUiModel.Reaction(count = 1, emoji = "❤️", reactedByUser = false)
                        ),
                        replyPreview = ReplyPreview(
                            messageId = "2",
                            title = "mrx.12",
                            content = ReplyPreview.Content.Text("Okay")
                        ),
                        origin = ChatMessageOrigin.User,
                        isEdited = true
                    ),
                    grouping = ChatMessageGrouping.Standalone,
                    showTimestamp = true,
                    isHighlighted = false,
                    onMessageAction = {},
                    onLongPress = {},
                    canBeReplied = false
                )

                TextMessage(
                    modifier = Modifier.fillMaxWidth(),
                    message = ChatMessageUiModel.Text(
                        id = "6",
                        text = "Hi!",
                        direction = ChatMessageUiModel.Direction.OUTGOING,
                        status = ChatMessageUiModel.Status.SENT,
                        timestamp = System.currentTimeMillis(),
                        reactions = persistentListOf(
                            ChatMessageUiModel.Reaction(count = 1, emoji = "❤️", reactedByUser = false),
                            ChatMessageUiModel.Reaction(count = 1, emoji = "❤️", reactedByUser = false),
                        ),
                        replyPreview = null,
                        origin = ChatMessageOrigin.User,
                        isEdited = false
                    ),
                    grouping = ChatMessageGrouping.Standalone,
                    showTimestamp = true,
                    isHighlighted = false,
                    onMessageAction = {},
                    onLongPress = {},
                    canBeReplied = false
                )
            }
        }
    }
}
