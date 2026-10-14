@file:OptIn(ExperimentalMaterial3Api::class)

package io.paritytech.polkadotapp.common.presentation.compose.video

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.size
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Slider
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import androidx.media3.common.Player
import androidx.media3.common.listen
import io.paritytech.polkadotapp.design.components.progress.NovaLinearProgressIndicator
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import kotlinx.coroutines.*

@Composable
fun PlayerSeekBar(
    modifier: Modifier,
    player: Player,
    onInteraction: () -> Unit = {}
) {
    val state = rememberPlayerPositionState(player)

    Slider(
        modifier = modifier,
        value = state.currentPosition.toFloat(),
        valueRange = 0f..state.totalDuration.toFloat(),
        onValueChange = { progress ->
            onInteraction()
            if (state.totalDuration > 0) {
                state.seekTo(progress.toLong())
            }
        },
        onValueChangeFinished = { onInteraction() },
        thumb = {
            Box(
                modifier = Modifier
                    .size(24.dp)
                    .clip(PolkadotTheme.shapes.full)
                    .background(Color.White)
            )
        },
        track = { sliderState ->
            NovaLinearProgressIndicator(
                modifier = Modifier.fillMaxWidth(),
                progress = sliderState.coercedValueAsFraction
            )
        }
    )
}

@Composable
private fun rememberPlayerPositionState(player: Player): PlayerProgressState {
    val state = remember { PlayerProgressState(player) }
    LaunchedEffect(player) {
        state.observe()
    }

    DisposableEffect(player) {
        state.startTracking()
        onDispose {
            state.stopTracking()
        }
    }

    return state
}

private class PlayerProgressState(private val player: Player) {
    private var positionUpdateJob: Job? = null
    var currentPosition by mutableLongStateOf(player.currentPosition.coerceAtLeast(0))
    var totalDuration by mutableLongStateOf(player.duration.coerceAtLeast(1L))
    var currentBufferedPosition by mutableLongStateOf(0L)

    fun seekTo(position: Long) {
        player.seekTo(position)
    }

    fun startTracking() {
        updatePlayingState()
    }

    fun stopTracking() {
        positionUpdateJob?.cancel()
        positionUpdateJob = null
    }

    suspend fun observe(): Nothing = player.listen { events ->
        if (events.containsAny(
                Player.EVENT_POSITION_DISCONTINUITY,
                Player.EVENT_PLAYBACK_STATE_CHANGED,
                Player.EVENT_PLAY_WHEN_READY_CHANGED,
                Player.EVENT_MEDIA_ITEM_TRANSITION,
                Player.EVENT_TIMELINE_CHANGED,
            )
        ) {
            totalDuration = player.duration.coerceAtLeast(1L)
            updatePosition()
        }

        if (events.containsAny(
                Player.EVENT_PLAYBACK_STATE_CHANGED,
                Player.EVENT_PLAY_WHEN_READY_CHANGED
            )
        ) {
            updatePlayingState()
        }
    }

    private fun updatePlayingState() {
        if (player.isPlaying) {
            startPositionUpdateJob()
        } else {
            positionUpdateJob?.cancel()
            positionUpdateJob = null
        }
    }

    private fun updatePosition() {
        currentPosition = player.currentPosition.coerceAtLeast(0)
        currentBufferedPosition = player.bufferedPosition
    }

    private fun startPositionUpdateJob() {
        positionUpdateJob = CoroutineScope(Dispatchers.Main).launch {
            while (isActive) {
                updatePosition()
                delay(500)
            }
        }
    }
}
