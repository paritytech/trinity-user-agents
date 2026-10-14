package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.core.net.toUri
import io.paritytech.polkadotapp.common.presentation.formatters.space.LocalInformationSizeFormatter
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.FileOutlined
import io.paritytech.polkadotapp.design.components.image.NovaAsyncImage
import io.paritytech.polkadotapp.design.components.image.rememberBlurHashPlaceholder
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageGrouping
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageSurfaceStyle
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel.Multimedia.MultimediaType
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel.ProductMedia.MediaItem
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageAction
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageLayoutInfo
import io.paritytech.polkadotapp.feature_chats_api.presentation.transfer.LocalMultimediaMessageController
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components.FlatMessageTimestamp
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.multimedia.VideoContent
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.multimedia.calculateMediaSize
import io.paritytech.polkadotapp.common.R as RCommon

private val FileIconSize = 24.dp

/** Product rich text with media items above the text (02-DESIGN §6.2). */
@Composable
fun ProductMediaMessage(
    modifier: Modifier = Modifier,
    message: ChatMessageUiModel.ProductMedia,
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
        replyPreview = message.replyPreview,
        surfaceStyle = surfaceStyle,
    ) {
        val mediaWidth = calculateMediaSize(MultimediaType.Image(height = 1, width = 1)).width

        Column(
            modifier = Modifier.width(mediaWidth),
            verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.extraTiny),
        ) {
            if (message.replyPreview != null) {
                VerticalSpacer { tiny }
            }

            message.media.forEach { item -> MediaItemContent(item = item) }

            val text = message.text
            if (text != null) {
                MarkdownMessageContent(
                    showTimestamp = showTimestamp,
                    message = message,
                    markdown = text,
                    isEdited = message.isEdited,
                    textColor = surfaceStyle.textColor ?: message.direction.defaultTextColor,
                )
            } else if (showTimestamp) {
                FlatMessageTimestamp(
                    modifier = Modifier
                        .align(Alignment.End)
                        .padding(horizontal = PolkadotTheme.spacings.small, vertical = PolkadotTheme.spacings.tiny),
                    message = message,
                )
            }
        }
    }
}

@Composable
private fun MediaItemContent(item: MediaItem) {
    val description = item.alt ?: if (item is MediaItem.Video) stringResource(RCommon.string.chat_message_video) else null
    val semanticsModifier = if (description != null) Modifier.semantics { contentDescription = description } else Modifier

    when (item) {
        is MediaItem.Image -> {
            val size = calculateMediaSize(MultimediaType.Image(height = item.height ?: 0, width = item.width ?: 0))
            val placeholder = rememberBlurHashPlaceholder(item.blurHash)
            val controller = LocalMultimediaMessageController.current
            val remoteUrl = item.model as? String

            NovaAsyncImage(
                modifier = semanticsModifier
                    .size(size)
                    .clickable(enabled = remoteUrl != null) { remoteUrl?.let { controller.openImage(it.toUri()) } },
                model = item.model,
                contentDescription = description,
                placeholder = placeholder,
                fallback = placeholder,
                contentScale = ContentScale.Crop,
            )
        }

        is MediaItem.Video -> VideoContent(
            modifier = semanticsModifier.size(calculateMediaSize(MultimediaType.Image(height = item.height ?: 0, width = item.width ?: 0))),
            uri = item.url?.toUri(),
            blurHash = item.blurHash,
            canPlay = item.url != null,
        )

        is MediaItem.File -> FileItem(modifier = semanticsModifier, item = item)
    }
}

@Composable
private fun FileItem(modifier: Modifier, item: MediaItem.File) {
    val sizeLabel = item.size?.let { LocalInformationSizeFormatter.current.format(it) }

    Row(
        modifier = modifier
            .fillMaxWidth()
            .padding(horizontal = PolkadotTheme.spacings.extraMedium, vertical = PolkadotTheme.spacings.small),
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.extraMedium),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        NovaIcon(
            modifier = Modifier.size(FileIconSize),
            imageVector = NovaIcons.FileOutlined,
            tint = PolkadotTheme.colors.fg.primary,
        )

        Column(modifier = Modifier.weight(1f)) {
            NovaText(
                text = item.fileName,
                style = PolkadotTheme.typography.body.mediumEmphasized,
                color = PolkadotTheme.colors.fg.primary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            NovaText(
                text = sizeLabel ?: item.mimeType,
                style = PolkadotTheme.typography.body.small,
                color = PolkadotTheme.colors.fg.secondary,
                maxLines = 1,
            )
        }
    }
}
