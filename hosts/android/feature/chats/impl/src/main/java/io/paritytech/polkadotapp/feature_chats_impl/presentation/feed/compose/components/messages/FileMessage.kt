package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import io.paritytech.polkadotapp.common.presentation.formatters.space.InformationSizeFormatter
import io.paritytech.polkadotapp.common.presentation.formatters.space.LocalInformationSizeFormatter
import io.paritytech.polkadotapp.common.utils.InformationSize.Companion.bytes
import io.paritytech.polkadotapp.common.utils.openPdf
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.FileOutlined
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageOrigin
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageGrouping
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageSurfaceStyle
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components.FlatMessageTimestamp
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components.TextMessageLayout
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.ChatMessageTimeFormatter
import io.paritytech.polkadotapp.feature_chats_impl.presentation.formatter.LocalChatMessageTimeFormatter
import java.util.UUID

@Composable
fun FileMessage(
    modifier: Modifier = Modifier,
    message: ChatMessageUiModel.File,
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
        val hasInlineTimestamp = message.text == null && showTimestamp
        val hasTextBelow = message.text != null

        Row {
            Column(
                modifier = Modifier.weight(1f)
            ) {
                FileMessageContent(
                    modifier = Modifier.padding(
                        start = PolkadotTheme.spacings.extraMedium,
                        end = if (hasInlineTimestamp) PolkadotTheme.spacings.zero else PolkadotTheme.spacings.extraMedium,
                        top = PolkadotTheme.spacings.small,
                        bottom = if (hasTextBelow) PolkadotTheme.spacings.zero else PolkadotTheme.spacings.small
                    ),
                    message = message
                )

                if (hasTextBelow) {
                    val timestampComposable = if (showTimestamp) {
                        @Composable {
                            FlatMessageTimestamp(message = message)
                        }
                    } else {
                        null
                    }

                    TextMessageLayout(
                        modifier = Modifier.fillMaxWidth(),
                        text = message.text.orEmpty(),
                        style = PolkadotTheme.typography.body.large,
                        color = message.direction.defaultTextColor,
                        timestamp = timestampComposable,
                        direction = message.direction
                    )
                }
            }

            if (hasInlineTimestamp) {
                FlatMessageTimestamp(
                    modifier = Modifier
                        .align(Alignment.Bottom)
                        .padding(
                            horizontal = PolkadotTheme.spacings.small,
                            vertical = PolkadotTheme.spacings.tiny
                        ),
                    message = message
                )
            }
        }
    }
}

@Composable
private fun FileMessageContent(
    modifier: Modifier = Modifier,
    message: ChatMessageUiModel.File
) {
    val sizeFormatter = LocalInformationSizeFormatter.current
    val sizeLabel = sizeFormatter.format(message.size)

    val context = LocalContext.current

    Row(
        modifier = Modifier
            .clickable(onClick = { context.openPdf(message.uri) })
            .then(modifier),
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.extraMedium),
        verticalAlignment = Alignment.CenterVertically
    ) {
        FileIcon(direction = message.direction)

        Column(
            modifier = Modifier.weight(1f)
        ) {
            NovaText(
                text = message.fileName,
                style = PolkadotTheme.typography.paragraph.large,
                color = message.direction.defaultTextColor,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis
            )

            NovaText(
                text = sizeLabel,
                style = PolkadotTheme.typography.body.smallEmphasized,
                color = message.direction.subtitleTextColor
            )
        }
    }
}

@Composable
private fun FileIcon(direction: ChatMessageUiModel.Direction) {
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
                .padding(PolkadotTheme.spacings.smallIncreased)
                .size(20.dp),
            imageVector = NovaIcons.FileOutlined,
            tint = iconColor
        )
    }
}

@Preview
@Composable
private fun FileMessagePreview() {
    PolkadotTheme {
        CompositionLocalProvider(
            LocalChatMessageTimeFormatter provides ChatMessageTimeFormatter.mocked(),
            LocalInformationSizeFormatter provides InformationSizeFormatter.mocked(),
            LocalChatFeedTimestampAnchor provides System.currentTimeMillis()
        ) {
            Column(
                verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.mediumIncreased)
            ) {
                FileMessage(
                    modifier = Modifier.fillMaxWidth(),
                    message = ChatMessageUiModel.File(
                        id = UUID.randomUUID().toString(),
                        timestamp = 0L,
                        direction = ChatMessageUiModel.Direction.INCOMING,
                        status = ChatMessageUiModel.Status.SENT,
                        fileName = "Stencil.PDF",
                        size = 390_000L.bytes,
                        thumbnailUri = null,
                        text = "What’s keeping you busy?",
                        origin = ChatMessageOrigin.User,
                        uri = "".toUri()
                    ),
                    showTimestamp = true,
                    grouping = ChatMessageGrouping.Standalone,
                    isHighlighted = false
                )

                FileMessage(
                    modifier = Modifier.fillMaxWidth(),
                    message = ChatMessageUiModel.File(
                        id = UUID.randomUUID().toString(),
                        timestamp = 0L,
                        direction = ChatMessageUiModel.Direction.OUTGOING,
                        status = ChatMessageUiModel.Status.READ,
                        fileName = "Long Document Name Very Very Long.pdf",
                        size = 1_250_000L.bytes,
                        thumbnailUri = null,
                        text = null,
                        origin = ChatMessageOrigin.User,
                        uri = "".toUri()
                    ),
                    showTimestamp = true,
                    grouping = ChatMessageGrouping.Standalone,
                    isHighlighted = false
                )
            }
        }
    }
}
