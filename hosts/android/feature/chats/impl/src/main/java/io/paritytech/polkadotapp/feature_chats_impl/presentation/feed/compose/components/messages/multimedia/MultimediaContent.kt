package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.multimedia

import android.net.Uri
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.DpSize
import androidx.compose.ui.unit.dp
import androidx.media3.ui.compose.ContentFrame
import io.paritytech.polkadotapp.common.presentation.compose.video.VideoPlayerControlsContainer
import io.paritytech.polkadotapp.common.presentation.compose.video.rememberExoPlayer
import io.paritytech.polkadotapp.common.presentation.compose.video.toDefaultMediaSource
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.VideoPlay
import io.paritytech.polkadotapp.design.components.image.NovaAsyncImage
import io.paritytech.polkadotapp.design.components.image.rememberBlurHashPlaceholder
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_chats_api.presentation.common.getMaxMessageHeight
import io.paritytech.polkadotapp.feature_chats_api.presentation.common.getMaxMessageWidth
import io.paritytech.polkadotapp.feature_chats_api.presentation.model.ChatMessageUiModel.Multimedia.MultimediaType

internal val MULTIMEDIA_CONTROL_DIAMETER = 60.dp
internal val MULTIMEDIA_CONTROL_ICON_SIZE = 30.dp

@Composable
internal fun ImagePreview(
    modifier: Modifier,
    uri: Uri?,
    blurHash: String?
) {
    val placeholder = rememberBlurHashPlaceholder(blurHash)

    NovaAsyncImage(
        modifier = modifier,
        model = uri,
        placeholder = placeholder,
        fallback = placeholder,
        contentScale = ContentScale.Crop
    )
}

@Composable
internal fun VideoContent(
    modifier: Modifier,
    uri: Uri?,
    blurHash: String?,
    canPlay: Boolean
) {
    var playerAttached by remember(uri) { mutableStateOf(false) }

    Box(modifier) {
        if (playerAttached && uri != null) {
            VideoPlayer(
                modifier = Modifier.fillMaxSize(),
                uri = uri
            )
        } else {
            ImagePreview(
                modifier = Modifier.fillMaxSize(),
                uri = uri,
                blurHash = blurHash
            )

            if (uri != null && canPlay) {
                PlayButton(
                    modifier = Modifier.align(Alignment.Center),
                    onClick = { playerAttached = true }
                )
            }
        }
    }
}

@Composable
private fun VideoPlayer(
    modifier: Modifier,
    uri: Uri
) {
    val context = LocalContext.current
    val mediaSource = remember(uri) { uri.toDefaultMediaSource(context) }
    val player = rememberExoPlayer(mediaSource = mediaSource, playWhenReady = true)

    VideoPlayerControlsContainer(
        modifier = modifier,
        player = player
    ) {
        ContentFrame(
            modifier = Modifier
                .fillMaxSize()
                .clipToBounds(),
            player = player,
            contentScale = ContentScale.Crop
        )
    }
}

@Composable
private fun PlayButton(
    modifier: Modifier,
    onClick: () -> Unit
) {
    Control(
        modifier = modifier,
        onClick = onClick
    ) {
        ControlIcon(NovaIcons.VideoPlay)
    }
}

@Composable
private fun ControlIcon(imageVector: ImageVector) {
    NovaIcon(
        modifier = Modifier.size(MULTIMEDIA_CONTROL_ICON_SIZE),
        imageVector = imageVector,
        tint = PolkadotTheme.colors.fg.primary
    )
}

@Composable
private fun Control(
    modifier: Modifier,
    onClick: (() -> Unit)? = null,
    content: @Composable BoxScope.() -> Unit
) {
    PolkadotSurface(
        modifier = modifier,
        shape = PolkadotTheme.shapes.full,
        color = PolkadotTheme.colors.bg.surface.overlay,
        onClick = onClick
    ) {
        Box(
            modifier = Modifier.size(MULTIMEDIA_CONTROL_DIAMETER),
            contentAlignment = Alignment.Center
        ) {
            content()
        }
    }
}

@Composable
internal fun calculateMediaSize(type: MultimediaType): DpSize {
    val maxWidth = getMaxMessageWidth()
    val maxHeight = getMaxMessageHeight()
    val ratio = type.aspectRatio()

    var width = maxWidth
    var height = maxWidth / ratio

    if (height > maxHeight) {
        height = maxHeight
        width = maxHeight * ratio
    }

    return DpSize(width, height)
}

private fun MultimediaType.aspectRatio(): Float {
    return when (this) {
        is MultimediaType.Image -> if (width > 0 && height > 0) {
            width.toFloat() / height.toFloat()
        } else {
            1f
        }
        is MultimediaType.Video -> 1f
    }
}
