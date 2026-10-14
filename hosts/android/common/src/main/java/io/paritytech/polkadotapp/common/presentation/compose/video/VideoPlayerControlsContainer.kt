package io.paritytech.polkadotapp.common.presentation.compose.video

import android.content.Context
import android.net.Uri
import androidx.annotation.OptIn
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import androidx.media3.common.util.UnstableApi
import androidx.media3.datasource.DataSource
import androidx.media3.datasource.DefaultDataSource
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.exoplayer.source.DefaultMediaSourceFactory
import androidx.media3.exoplayer.source.MediaSource
import androidx.media3.exoplayer.source.ProgressiveMediaSource
import androidx.media3.ui.compose.state.rememberPlayPauseButtonState
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.VideoPause
import io.paritytech.polkadotapp.design.components.icon.vectors.VideoPlay
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import kotlinx.coroutines.delay

private const val ControlsVisibilityDelayMillis = 3000L

@OptIn(UnstableApi::class)
@Composable
fun VideoPlayerControlsContainer(
    player: Player?,
    modifier: Modifier = Modifier,
    onPlayingChanged: (Boolean) -> Unit = {},
    content: @Composable () -> Unit
) {
    var controlsVisible by remember { mutableStateOf(true) }
    val playPauseButtonState = player?.let { rememberPlayPauseButtonState(it) }
    val isPlaying = playPauseButtonState?.showPlay?.not() ?: false

    LaunchedEffect(isPlaying) {
        onPlayingChanged(isPlaying)
    }

    val controlsAlpha by animateFloatAsState(
        targetValue = if (controlsVisible || !isPlaying) 1f else 0f,
        label = "controlsAlpha"
    )

    LaunchedEffect(controlsVisible, isPlaying) {
        if (controlsVisible && isPlaying) {
            delay(ControlsVisibilityDelayMillis)
            controlsVisible = false
        }
    }

    Box(
        modifier = modifier
            .clickable(
                interactionSource = remember { MutableInteractionSource() },
                indication = null
            ) {
                controlsVisible = !controlsVisible
            }
    ) {
        content()

        Box(
            modifier = Modifier
                .fillMaxSize()
                .alpha(controlsAlpha)
        ) {
            if (player != null && playPauseButtonState != null) {
                PolkadotSurface(
                    modifier = Modifier.align(Alignment.Center),
                    shape = PolkadotTheme.shapes.full,
                    color = PolkadotTheme.colors.bg.surface.overlay,
                    onClick = {
                        playPauseButtonState.onClick()
                        controlsVisible = true
                    }
                ) {
                    Box(
                        modifier = Modifier.size(60.dp),
                        contentAlignment = Alignment.Center
                    ) {
                        NovaIcon(
                            modifier = Modifier.size(30.dp),
                            imageVector = if (playPauseButtonState.showPlay) NovaIcons.VideoPlay else NovaIcons.VideoPause,
                            tint = PolkadotTheme.colors.fg.primary
                        )
                    }
                }

                PlayerSeekBar(
                    modifier = Modifier
                        .fillMaxWidth()
                        .align(Alignment.BottomCenter)
                        .padding(
                            horizontal = PolkadotTheme.spacings.small
                        ),
                    player = player,
                    onInteraction = { controlsVisible = true }
                )
            }
        }
    }
}

fun Uri.toProgressiveMediaSource(context: Context): MediaSource {
    return toProgressiveMediaSource(DefaultDataSource.Factory(context))
}

fun Uri.toProgressiveMediaSource(dataSourceFactory: DataSource.Factory): MediaSource {
    return ProgressiveMediaSource.Factory(dataSourceFactory).createMediaSource(MediaItem.fromUri(this))
}

fun Uri.toDefaultMediaSource(context: Context): MediaSource {
    return DefaultMediaSourceFactory(context).createMediaSource(MediaItem.fromUri(this))
}

@Composable
fun rememberExoPlayer(
    mediaSource: MediaSource?,
    playWhenReady: Boolean = true,
): ExoPlayer {
    val context = LocalContext.current
    val player = remember { ExoPlayer.Builder(context).build() }

    DisposableEffect(player) {
        onDispose { player.release() }
    }

    LaunchedEffect(mediaSource) {
        if (mediaSource != null && player.currentMediaItem == null) {
            player.setMediaSource(mediaSource)
            player.prepare()
            player.playWhenReady = playWhenReady
        }
    }

    return player
}
