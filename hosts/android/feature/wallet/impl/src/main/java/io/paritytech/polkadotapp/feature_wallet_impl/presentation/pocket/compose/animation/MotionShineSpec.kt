package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.animation

import androidx.compose.ui.geometry.Offset

// Templated into the AGSL shader (API 33+). The Brush fallback (API < 33) slides along the main axis only.

// Main shine axis: 151.367deg (screen space, y-down) and its perpendicular.
internal val ShineMainAxis = Offset(0.4787f, 0.8780f)
internal val ShinePerpAxis = Offset(-0.8780f, 0.4787f)

// How far a full tilt slides the shine along each axis.
internal const val SHINE_MAIN_TILT_X = 0.16f
internal const val SHINE_MAIN_TILT_Y = 0.10f
internal const val SHINE_PERP_TILT_X = 0.12f
internal const val SHINE_PERP_TILT_Y = 0.18f

internal data class MotionShineParameters(
    val intensity: Float,
    val dimming: Float,
    val width: Float,
    val length: Float,
    val center: Float
) {
    companion object {
        val DigitalDollarCard = MotionShineParameters(
            intensity = 0.5f,
            dimming = 0.7f,
            width = 0.3f,
            length = 0.8f,
            center = 0.3f
        )

        fun collectibles(isExpanded: Boolean) = MotionShineParameters(
            intensity = 0.08f,
            dimming = 0.17f,
            width = 0.1f,
            length = 0.5f,
            center = if (isExpanded) 0.5f else 0.25f
        )
    }
}
