package io.paritytech.polkadotapp.tools_media_connection_api.domain.models

import android.os.Build

/**
 * Visual effect applied to a [VideoTrack] at the renderer level (a Compose-layer effect cannot
 * capture the separately-composited video surface). An effect silently no-ops on devices where
 * [isSupported] is false — callers for whom the effect is a hard requirement (e.g. a privacy
 * blur) must check [isSupported] and provide their own fallback.
 */
sealed interface VideoTrackEffect {
    val isSupported: Boolean

    data object Blur : VideoTrackEffect {
        override val isSupported: Boolean = Build.VERSION.SDK_INT >= Build.VERSION_CODES.S
    }
}
