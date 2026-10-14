package io.paritytech.polkadotapp.feature_calls_impl.presentation.call.compose.components

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.SubcomposeLayout
import androidx.compose.ui.res.stringResource
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.CallEndFilled
import io.paritytech.polkadotapp.design.components.icon.vectors.CallFilled
import io.paritytech.polkadotapp.design.components.icon.vectors.MicOffFilled
import io.paritytech.polkadotapp.design.components.icon.vectors.VideocamFilled
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_calls_impl.media.CallAudioDeviceType
import io.paritytech.polkadotapp.feature_calls_impl.presentation.call.models.CallUiState
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun CallControlsRow(
    state: CallUiState,
    onAccept: () -> Unit,
    onAudioClick: () -> Unit,
    onToggleCamera: () -> Unit,
    onToggleMicrophone: () -> Unit,
    onEndCall: () -> Unit,
) {
    val inProgress = state as? CallUiState.InProgress

    CallControlsLayout(
        modifier = Modifier
            .fillMaxWidth()
            .padding(
                vertical = PolkadotTheme.spacings.mediumIncreased,
                horizontal = PolkadotTheme.spacings.large,
            )
            .navigationBarsPadding()
    ) {
        if (state is CallUiState.Incoming) {
            CallActionButton(
                onClick = onAccept,
                text = stringResource(RCommon.string.call_accept),
                icon = NovaIcons.CallFilled,
                color = PolkadotTheme.colors.fg.success
            )
        }

        if (inProgress != null) {
            val selectedAudioType = inProgress.audioDevices.firstOrNull { it.isSelected }?.type

            CallControlButton(
                onClick = onAudioClick,
                text = stringResource(RCommon.string.call_speaker),
                icon = audioDeviceIcon(selectedAudioType),
                active = selectedAudioType != null && selectedAudioType != CallAudioDeviceType.Earpiece,
            )

            CallControlButton(
                onClick = onToggleCamera,
                text = stringResource(RCommon.string.call_video),
                icon = NovaIcons.VideocamFilled,
                active = inProgress.cameraOn,
            )

            CallControlButton(
                onClick = onToggleMicrophone,
                text = stringResource(RCommon.string.call_mute),
                icon = NovaIcons.MicOffFilled,
                active = inProgress.micMuted,
            )
        }

        if (state !is CallUiState.Terminal) {
            CallActionButton(
                onClick = onEndCall,
                text = stringResource(RCommon.string.call_end),
                icon = NovaIcons.CallEndFilled,
                color = PolkadotTheme.colors.fg.error
            )
        }
    }
}

@Composable
private fun CallControlsLayout(
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    SubcomposeLayout(modifier = modifier) { constraints ->
        val anchorHeight = subcompose(SLOT_ANCHOR) {
            Box(modifier = Modifier.alpha(0f)) {
                CallActionButton(
                    onClick = {},
                    text = "",
                    icon = NovaIcons.CallEndFilled,
                    color = Color.Transparent
                )
            }
        }.first().measure(constraints).height

        val itemConstraints = constraints.copy(minWidth = 0, minHeight = 0)
        val placeables = subcompose(SLOT_CONTENT, content).map { it.measure(itemConstraints) }

        val rowHeight = maxOf(anchorHeight, placeables.maxOfOrNull { it.height } ?: 0)
        val rowWidth = constraints.maxWidth

        layout(rowWidth, rowHeight) {
            when (placeables.size) {
                0 -> Unit

                1 -> {
                    val p = placeables[0]
                    p.place(x = rowWidth - p.width, y = (rowHeight - p.height) / 2)
                }

                else -> {
                    val totalContentWidth = placeables.sumOf { it.width }
                    val gap = ((rowWidth - totalContentWidth) / (placeables.size - 1))
                        .coerceAtLeast(0)
                    var x = 0
                    placeables.forEach { p ->
                        p.place(x = x, y = (rowHeight - p.height) / 2)
                        x += p.width + gap
                    }
                }
            }
        }
    }
}

private const val SLOT_ANCHOR = "anchor"
private const val SLOT_CONTENT = "content"
