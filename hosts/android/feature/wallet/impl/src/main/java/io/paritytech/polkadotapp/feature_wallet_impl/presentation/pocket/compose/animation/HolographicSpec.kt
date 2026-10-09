package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.animation

import androidx.compose.ui.geometry.Offset

// Tilt geometry shared by the motion shine's AGSL shader (API 33+, source templated from these
// values) and its Brush fallback (API < 33).

// Main gradient axis: 151.367deg (screen space, y-down) and its perpendicular.
internal val HoloMainAxis = Offset(0.4787f, 0.8780f)
internal val HoloPerpAxis = Offset(-0.8780f, 0.4787f)

// Tilt drift factors of the ramp (t) and the perpendicular highlight (sShift).
internal const val HOLO_RAMP_TILT_X = 0.16f
internal const val HOLO_RAMP_TILT_Y = 0.10f
internal const val HOLO_HIGHLIGHT_TILT_X = 0.12f
internal const val HOLO_HIGHLIGHT_TILT_Y = 0.18f
