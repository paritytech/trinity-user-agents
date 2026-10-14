package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.QuestionAnswer
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageGrouping
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageSurfaceStyle
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageAction
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageLayoutInfo
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.ChatMessageTimeFormatter
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.LocalChatMessageTimeFormatter
import kotlinx.collections.immutable.persistentListOf
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun ChatRequestMessage(
    modifier: Modifier = Modifier,
    message: ChatMessageUiModel.ChatRequest,
    showTimestamp: Boolean,
    grouping: ChatMessageGrouping,
    isHighlighted: Boolean,
    canBeReplied: Boolean,
    onMessageAction: (MessageAction) -> Unit,
    onLongPress: (MessageLayoutInfo) -> Unit,
    customBubbleStyle: ChatMessageSurfaceStyle? = null,
) {
    val welcomeText = message.welcomeText

    Column(modifier = modifier) {
        if (message.direction == ChatMessageUiModel.Direction.OUTGOING) {
            ChatRequestLabel(modifier = Modifier.align(Alignment.CenterHorizontally))
        }

        if (welcomeText != null) {
            VerticalSpacer { small }

            ChatMessageContainer(
                message = message,
                grouping = grouping,
                isHighlighted = isHighlighted,
                canBeReplied = canBeReplied,
                onMessageAction = onMessageAction,
                onLongPress = onLongPress,
                reactions = message.reactions,
                surfaceStyle = customBubbleStyle ?: ChatMessageSurfaceStyle.default(message.direction),
            ) {
                TextMessageContent(
                    showTimestamp = showTimestamp,
                    message = message,
                    text = welcomeText,
                    isEdited = false
                )
            }
        }
    }
}

@Composable
private fun ChatRequestLabel(
    modifier: Modifier = Modifier
) {
    Row(
        modifier = modifier,
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small, Alignment.CenterHorizontally),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        NovaIcon(
            modifier = Modifier.size(24.dp),
            imageVector = NovaIcons.QuestionAnswer,
            tint = PolkadotTheme.colors.fg.tertiary
        )

        NovaText(
            text = stringResource(RCommon.string.chat_request_message_sent),
            style = PolkadotTheme.typography.body.mediumEmphasized,
            color = PolkadotTheme.colors.fg.tertiary,
            textAlign = TextAlign.Center
        )
    }
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun ChatRequestMessagePreview() {
    PolkadotTheme {
        CompositionLocalProvider(
            LocalChatMessageTimeFormatter provides ChatMessageTimeFormatter.mocked()
        ) {
            ChatRequestMessage(
                message = ChatMessageUiModel.ChatRequest(
                    id = "1",
                    timestamp = System.currentTimeMillis(),
                    direction = ChatMessageUiModel.Direction.OUTGOING,
                    status = ChatMessageUiModel.Status.SENT,
                    origin = ChatMessageOrigin.User,
                    welcomeText = "Hi! Here's the update we discussed. Let me know if you have any questions.",
                    reactions = persistentListOf()
                ),
                showTimestamp = true,
                grouping = ChatMessageGrouping.Standalone,
                isHighlighted = false,
                onMessageAction = {},
                onLongPress = {},
                canBeReplied = true
            )
        }
    }
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun ChatRequestMessagePreview_NoText() {
    PolkadotTheme {
        ChatRequestMessage(
            message = ChatMessageUiModel.ChatRequest(
                id = "1",
                timestamp = System.currentTimeMillis(),
                direction = ChatMessageUiModel.Direction.OUTGOING,
                status = ChatMessageUiModel.Status.SENT,
                origin = ChatMessageOrigin.User,
                welcomeText = null,
                reactions = persistentListOf()
            ),
            showTimestamp = true,
            grouping = ChatMessageGrouping.Standalone,
            isHighlighted = false,
            onMessageAction = {},
            onLongPress = {},
            canBeReplied = true
        )
    }
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun ChatRequestMessagePreview_WithReactions() {
    PolkadotTheme {
        CompositionLocalProvider(
            LocalChatMessageTimeFormatter provides ChatMessageTimeFormatter.mocked()
        ) {
            ChatRequestMessage(
                message = ChatMessageUiModel.ChatRequest(
                    id = "1",
                    timestamp = System.currentTimeMillis(),
                    direction = ChatMessageUiModel.Direction.INCOMING,
                    status = ChatMessageUiModel.Status.SENT,
                    origin = ChatMessageOrigin.User,
                    welcomeText = "Hi! Here's the update we discussed.",
                    reactions = persistentListOf(
                        ChatMessageUiModel.Reaction(count = 2, emoji = "👍", reactedByUser = true),
                        ChatMessageUiModel.Reaction(count = 1, emoji = "❤️", reactedByUser = false)
                    )
                ),
                showTimestamp = true,
                grouping = ChatMessageGrouping.Standalone,
                isHighlighted = false,
                onMessageAction = {},
                onLongPress = {},
                canBeReplied = true
            )
        }
    }
}
