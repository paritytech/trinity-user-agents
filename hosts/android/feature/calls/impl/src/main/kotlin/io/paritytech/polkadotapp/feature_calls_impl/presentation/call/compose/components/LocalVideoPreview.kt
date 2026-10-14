package io.paritytech.polkadotapp.feature_calls_impl.presentation.call.compose.components

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.VideoTrack

@Composable
fun LocalVideoPreview(
    modifier: Modifier = Modifier,
    videoTrack: VideoTrack,
) {
    PolkadotSurface(
        modifier = modifier,
        shape = PolkadotTheme.shapes.mediumIncreased,
        color = Color.Black,
    ) {
        videoTrack.Render(
            modifier = Modifier.fillMaxSize(),
            isMirrored = true,
        )
    }
}
