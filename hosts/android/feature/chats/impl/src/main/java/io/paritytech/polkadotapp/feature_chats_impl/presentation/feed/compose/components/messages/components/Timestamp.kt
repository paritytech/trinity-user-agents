package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.icons.MessageStatusFailed
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.icons.MessageStatusPending
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.icons.MessageStatusRead
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.icons.MessageStatusSent
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.ChatMessageTimeFormatter
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.LocalChatMessageTimeFormatter
import kotlinx.collections.immutable.persistentListOf

@Composable
fun FlatMessageTimestamp(
    modifier: Modifier = Modifier,
    message: ChatMessageUiModel,
) {
    val contentColor = when (message.direction) {
        ChatMessageUiModel.Direction.INCOMING -> PolkadotTheme.colors.fg.tertiary
        ChatMessageUiModel.Direction.OUTGOING -> PolkadotTheme.colors.fg.secondaryInverted
    }

    MessageTimestampContent(
        modifier = modifier,
        message = message,
        textColor = contentColor,
        iconColor = contentColor,
    )
}

@Composable
fun FilledMessageTimestamp(
    modifier: Modifier = Modifier,
    message: ChatMessageUiModel,
) {
    PolkadotSurface(
        modifier = modifier,
        shape = PolkadotTheme.shapes.full,
        color = PolkadotTheme.colors.bg.surface.overlay
    ) {
        val statusVisible = message.direction == ChatMessageUiModel.Direction.OUTGOING

        MessageTimestampContent(
            modifier = Modifier.padding(
                start = PolkadotTheme.spacings.small,
                end = if (statusVisible) PolkadotTheme.spacings.tiny else PolkadotTheme.spacings.small,
                top = PolkadotTheme.spacings.extraTiny,
                bottom = PolkadotTheme.spacings.extraTiny
            ),
            message = message,
            textColor = PolkadotTheme.colors.fg.staticWhite,
            iconColor = PolkadotTheme.colors.fg.staticWhite,
        )
    }
}

@Composable
private fun MessageTimestampContent(
    modifier: Modifier = Modifier,
    message: ChatMessageUiModel,
    textColor: Color,
    iconColor: Color,
) {
    val formatter = LocalChatMessageTimeFormatter.current

    Row(
        modifier = modifier,
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.tiny)
    ) {
        NovaText(
            text = formatter.formatMessageTime(message.timestamp),
            style = PolkadotTheme.typography.body.smallEmphasized,
            color = textColor,
            maxLines = 1
        )

        if (message.direction == ChatMessageUiModel.Direction.OUTGOING) {
            val isFailed = message.status == ChatMessageUiModel.Status.FAILED

            NovaIcon(
                modifier = Modifier.size(12.dp),
                imageVector = when (message.status) {
                    ChatMessageUiModel.Status.PENDING -> MessageStatusPending
                    ChatMessageUiModel.Status.SENT -> MessageStatusSent
                    ChatMessageUiModel.Status.READ -> MessageStatusRead
                    ChatMessageUiModel.Status.FAILED -> MessageStatusFailed
                },
                tint = if (isFailed) PolkadotTheme.colors.fg.error else iconColor
            )
        }
    }
}

@Preview(showBackground = true, backgroundColor = 0xFF000000)
@Composable
private fun MessageTimestampPreview() {
    PolkadotTheme {
        CompositionLocalProvider(
            LocalChatMessageTimeFormatter provides ChatMessageTimeFormatter.mocked()
        ) {
            Column(
                verticalArrangement = Arrangement.spacedBy(16.dp)
            ) {
                val messageIncoming = ChatMessageUiModel.Text(
                    id = "1",
                    text = "Hello!",
                    direction = ChatMessageUiModel.Direction.OUTGOING,
                    status = ChatMessageUiModel.Status.SENT,
                    timestamp = System.currentTimeMillis(),
                    reactions = persistentListOf(),
                    origin = ChatMessageOrigin.User,
                    replyPreview = null,
                    isEdited = false
                )

                ChatMessageSurface(
                    direction = messageIncoming.direction,
                ) {
                    Box(Modifier.padding(8.dp)) {
                        FlatMessageTimestamp(message = messageIncoming)
                    }
                }

                val messageOutgoing = ChatMessageUiModel.Text(
                    id = "2",
                    text = "Hi! How are you doing today?",
                    direction = ChatMessageUiModel.Direction.OUTGOING,
                    status = ChatMessageUiModel.Status.READ,
                    timestamp = System.currentTimeMillis(),
                    reactions = persistentListOf(),
                    origin = ChatMessageOrigin.User,
                    replyPreview = null,
                    isEdited = false
                )

                ChatMessageSurface(
                    direction = messageOutgoing.direction,
                ) {
                    Box(Modifier.padding(8.dp)) {
                        FlatMessageTimestamp(message = messageOutgoing)
                    }
                }

                ChatMessageSurface(
                    direction = messageOutgoing.direction,
                ) {
                    Box(Modifier.padding(8.dp)) {
                        FilledMessageTimestamp(message = messageOutgoing)
                    }
                }
            }
        }
    }
}
