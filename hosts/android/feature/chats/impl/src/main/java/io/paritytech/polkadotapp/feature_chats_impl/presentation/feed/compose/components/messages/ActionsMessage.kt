package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.tooling.preview.Preview
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotButtonSize
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageGrouping
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageSurfaceStyle
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageAction
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageLayoutInfo
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components.FlatMessageTimestamp
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components.TextMessageLayout
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.ChatMessageTimeFormatter
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.LocalChatMessageTimeFormatter
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.persistentListOf

private const val GRID_COLUMNS = 2

/**
 * Host-drawn buttons of a product under an optional text (02-DESIGN §6.3). A press goes to the product as
 * `ActionTriggered`. The host disables no button itself: the product edits the message to change them.
 */
@Composable
fun ActionsMessage(
    modifier: Modifier = Modifier,
    message: ChatMessageUiModel.Actions,
    showTimestamp: Boolean,
    grouping: ChatMessageGrouping,
    isHighlighted: Boolean,
    canBeReplied: Boolean,
    onMessageAction: (MessageAction) -> Unit,
    onLongPress: (MessageLayoutInfo) -> Unit,
    customBubbleStyle: ChatMessageSurfaceStyle? = null,
) {
    val surfaceStyle = customBubbleStyle ?: ChatMessageSurfaceStyle.default(message.direction)

    ChatMessageContainer(
        modifier = modifier,
        message = message,
        grouping = grouping,
        isHighlighted = isHighlighted,
        canBeReplied = canBeReplied,
        onMessageAction = onMessageAction,
        onLongPress = onLongPress,
        reactions = message.reactions,
        surfaceStyle = surfaceStyle,
    ) {
        val timestamp: (@Composable () -> Unit)? = if (showTimestamp) {
            { FlatMessageTimestamp(message = message) }
        } else {
            null
        }

        val text = message.text
        if (text != null) {
            TextMessageLayout(
                modifier = Modifier.fillMaxWidth(),
                text = text,
                style = PolkadotTheme.typography.body.large,
                color = surfaceStyle.textColor ?: message.direction.defaultTextColor,
                direction = message.direction,
                timestamp = timestamp.takeIf { message.buttons.isEmpty() },
            )
        }

        ActionButtons(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = PolkadotTheme.spacings.small)
                .padding(bottom = PolkadotTheme.spacings.small, top = if (message.text == null) PolkadotTheme.spacings.small else PolkadotTheme.spacings.zero),
            buttons = message.buttons,
            layout = message.layout,
            onPress = { button -> onMessageAction(MessageAction.ActionPress(message, button.actionId)) },
        )

        if (message.buttons.isNotEmpty() && timestamp != null) {
            Row(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = PolkadotTheme.spacings.small)
                    .padding(bottom = PolkadotTheme.spacings.small),
                horizontalArrangement = Arrangement.End,
            ) {
                timestamp()
            }
        }
    }
}

@Composable
private fun ActionButtons(
    modifier: Modifier,
    buttons: ImmutableList<ChatMessageUiModel.Actions.ActionButton>,
    layout: ChatMessageUiModel.Actions.ActionsLayout,
    onPress: (ChatMessageUiModel.Actions.ActionButton) -> Unit,
) {
    val rows = when (layout) {
        ChatMessageUiModel.Actions.ActionsLayout.COLUMN -> buttons.map { listOf(it) }
        ChatMessageUiModel.Actions.ActionsLayout.GRID -> buttons.chunked(GRID_COLUMNS)
    }

    Column(
        modifier = modifier,
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
    ) {
        rows.forEach { row ->
            Row(horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small)) {
                row.forEach { button ->
                    PolkadotTextButton(
                        modifier = Modifier.weight(1f),
                        text = button.title,
                        style = PolkadotButtonStyle.secondary(),
                        size = PolkadotButtonSize.medium(),
                        onClick = { onPress(button) },
                    )
                }
                if (row.size < GRID_COLUMNS && layout == ChatMessageUiModel.Actions.ActionsLayout.GRID) {
                    Box(modifier = Modifier.weight(1f))
                }
            }
        }
    }
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun ActionsMessagePreview() {
    PolkadotTheme {
        CompositionLocalProvider(LocalChatMessageTimeFormatter provides ChatMessageTimeFormatter.mocked()) {
            Column(verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.medium)) {
                ActionsMessage(
                    message = previewActions(ChatMessageUiModel.Actions.ActionsLayout.COLUMN),
                    showTimestamp = true,
                    grouping = ChatMessageGrouping.Standalone,
                    isHighlighted = false,
                    canBeReplied = true,
                    onMessageAction = {},
                    onLongPress = {},
                )
                ActionsMessage(
                    message = previewActions(ChatMessageUiModel.Actions.ActionsLayout.GRID),
                    showTimestamp = true,
                    grouping = ChatMessageGrouping.Standalone,
                    isHighlighted = false,
                    canBeReplied = true,
                    onMessageAction = {},
                    onLongPress = {},
                )
            }
        }
    }
}

private fun previewActions(layout: ChatMessageUiModel.Actions.ActionsLayout) = ChatMessageUiModel.Actions(
    id = layout.name,
    timestamp = 0L,
    direction = ChatMessageUiModel.Direction.INCOMING,
    status = ChatMessageUiModel.Status.SENT,
    origin = ChatMessageOrigin.Extension("ProductBot_weather"),
    text = "Pick a city",
    buttons = persistentListOf(
        ChatMessageUiModel.Actions.ActionButton("berlin", "Berlin"),
        ChatMessageUiModel.Actions.ActionButton("paris", "Paris"),
        ChatMessageUiModel.Actions.ActionButton("lisbon", "Lisbon"),
    ),
    layout = layout,
    reactions = persistentListOf(),
)
