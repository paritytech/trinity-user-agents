package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages

import android.net.Uri
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.common.presentation.formatters.space.LocalInformationSizeFormatter
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.FileOutlined
import io.paritytech.polkadotapp.design.components.icon.vectors.Retry
import io.paritytech.polkadotapp.design.components.image.NovaAsyncImage
import io.paritytech.polkadotapp.design.components.image.rememberBlurHashPlaceholder
import io.paritytech.polkadotapp.design.components.progress.NovaCircularProgressIndicator
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageGrouping
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageSurfaceStyle
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel.Multimedia.MultimediaType
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel.ProductMedia.MediaItem
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageAction
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.MessageLayoutInfo
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components.FlatMessageTimestamp
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.multimedia.VideoContent
import io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.multimedia.calculateMediaSize
import io.paritytech.polkadotapp.feature_chats_impl.presentation.resources.LocalProductMediaController
import kotlinx.coroutines.launch
import java.io.File
import io.paritytech.polkadotapp.common.R as RCommon

private val FileIconSize = 24.dp
private val RetryButtonSize = 48.dp
private val ProgressStrokeWidth = 2.dp

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
        is MediaItem.Image -> ImageItem(
            modifier = semanticsModifier.size(calculateMediaSize(MultimediaType.Image(height = item.height ?: 0, width = item.width ?: 0))),
            item = item,
            description = description,
        )

        is MediaItem.Video -> VideoItem(
            modifier = semanticsModifier.size(calculateMediaSize(MultimediaType.Image(height = item.height ?: 0, width = item.width ?: 0))),
            item = item,
        )

        is MediaItem.File -> FileItem(modifier = semanticsModifier, item = item)
    }
}

/** The image draws when its bytes are in the cache. A tap opens it full screen. */
@Composable
private fun ImageItem(modifier: Modifier, item: MediaItem.Image, description: String?) {
    val controller = LocalProductMediaController.current
    val media = rememberMediaFile(item)
    val placeholder = rememberBlurHashPlaceholder(item.blurHash)
    val file = (media.state as? MediaFileState.Ready)?.file

    Box(modifier = modifier) {
        NovaAsyncImage(
            modifier = Modifier
                .fillMaxSize()
                .clickable(
                    enabled = file != null,
                    onClickLabel = stringResource(RCommon.string.chat_media_open_image),
                ) { file?.let(controller::openImage) },
            model = file,
            contentDescription = description,
            placeholder = placeholder,
            fallback = placeholder,
            error = placeholder,
            contentScale = ContentScale.Crop,
        )

        if (media.state == MediaFileState.Failed) {
            MediaRetryButton(modifier = Modifier.align(Alignment.Center), onClick = media.retry)
        }
    }
}

/** The video plays in the bubble when its bytes are in the cache. Until then the bubble shows its poster. */
@Composable
private fun VideoItem(modifier: Modifier, item: MediaItem.Video) {
    val media = rememberMediaFile(item)
    val file = (media.state as? MediaFileState.Ready)?.file

    Box(modifier = modifier) {
        VideoContent(
            modifier = Modifier.fillMaxSize(),
            uri = file?.let(Uri::fromFile),
            blurHash = item.blurHash,
            canPlay = file != null,
        )

        if (media.state == MediaFileState.Failed) {
            MediaRetryButton(modifier = Modifier.align(Alignment.Center), onClick = media.retry)
        }
    }
}

/** A tap loads the file and lets the user pick an app that opens it. A file that does not load offers a retry. */
@Composable
private fun FileItem(modifier: Modifier, item: MediaItem.File) {
    val controller = LocalProductMediaController.current
    val scope = rememberCoroutineScope()
    var state by remember(item.resource) { mutableStateOf<FileOpenState>(FileOpenState.Idle) }
    val sizeLabel = item.size?.let { LocalInformationSizeFormatter.current.format(it) }

    Row(
        modifier = modifier
            .fillMaxWidth()
            .clickable(
                enabled = state != FileOpenState.Loading,
                onClickLabel = stringResource(RCommon.string.chat_media_open_file),
            ) {
                scope.launch {
                    state = FileOpenState.Loading
                    state = controller.load(item.resource, item.mimeType).fold(
                        onSuccess = { file ->
                            controller.openFile(file, item.mimeType)
                            FileOpenState.Idle
                        },
                        onFailure = { FileOpenState.Failed },
                    )
                }
            }
            .padding(horizontal = PolkadotTheme.spacings.extraMedium, vertical = PolkadotTheme.spacings.small),
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.extraMedium),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (state == FileOpenState.Loading) {
            NovaCircularProgressIndicator(modifier = Modifier.size(FileIconSize), strokeWidth = ProgressStrokeWidth)
        } else {
            NovaIcon(
                modifier = Modifier.size(FileIconSize),
                imageVector = if (state == FileOpenState.Failed) NovaIcons.Retry else NovaIcons.FileOutlined,
                tint = PolkadotTheme.colors.fg.primary,
            )
        }

        Column(modifier = Modifier.weight(1f)) {
            NovaText(
                text = item.fileName,
                style = PolkadotTheme.typography.body.mediumEmphasized,
                color = PolkadotTheme.colors.fg.primary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            NovaText(
                text = if (state == FileOpenState.Failed) stringResource(RCommon.string.chat_media_failed) else sizeLabel ?: item.mimeType,
                style = PolkadotTheme.typography.body.small,
                color = PolkadotTheme.colors.fg.secondary,
                maxLines = 2,
            )
        }
    }
}

@Composable
private fun MediaRetryButton(modifier: Modifier, onClick: () -> Unit) {
    val label = stringResource(RCommon.string.chat_media_failed)

    PolkadotSurface(
        modifier = modifier.semantics { contentDescription = label },
        shape = PolkadotTheme.shapes.full,
        color = PolkadotTheme.colors.bg.surface.overlay,
        onClick = onClick,
    ) {
        Box(modifier = Modifier.size(RetryButtonSize), contentAlignment = Alignment.Center) {
            NovaIcon(
                modifier = Modifier.size(FileIconSize),
                imageVector = NovaIcons.Retry,
                tint = PolkadotTheme.colors.fg.primary,
            )
        }
    }
}

private sealed interface MediaFileState {
    data object Loading : MediaFileState

    data class Ready(val file: File) : MediaFileState

    /** The bytes did not load, for example because the preimage lookup failed (02-DESIGN §6.2). */
    data object Failed : MediaFileState
}

private class MediaFileHandle(val state: MediaFileState, val retry: () -> Unit)

/** Loads the bytes of [item] into the cache when the item shows. A retry loads them again. */
@Composable
private fun rememberMediaFile(item: MediaItem): MediaFileHandle {
    val controller = LocalProductMediaController.current
    var attempt by remember(item.resource) { mutableIntStateOf(0) }
    val state by produceState<MediaFileState>(MediaFileState.Loading, item.resource, attempt) {
        value = MediaFileState.Loading
        value = controller.load(item.resource, item.mimeType).fold(
            onSuccess = { MediaFileState.Ready(it) },
            onFailure = { MediaFileState.Failed },
        )
    }

    return MediaFileHandle(state = state, retry = { attempt++ })
}

private sealed interface FileOpenState {
    data object Idle : FileOpenState

    data object Loading : FileOpenState

    data object Failed : FileOpenState
}
