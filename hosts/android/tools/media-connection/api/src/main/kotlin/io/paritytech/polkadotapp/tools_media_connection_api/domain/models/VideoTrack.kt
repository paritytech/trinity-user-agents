package io.paritytech.polkadotapp.tools_media_connection_api.domain.models

import android.graphics.Bitmap
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import kotlinx.collections.immutable.ImmutableSet
import kotlinx.collections.immutable.persistentSetOf

interface VideoTrack {
    @Composable
    fun Render(
        modifier: Modifier,
        isMirrored: Boolean,
        effects: ImmutableSet<VideoTrackEffect> = persistentSetOf(),
        onFirstFrameRendered: (() -> Unit)? = null,
        onFrameResolutionChanged: ((videoWidth: Int, videoHeight: Int, rotation: Int) -> Unit)? = null
    )

    fun captureFrame(): Bitmap?

    fun dispose()
}
