package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.animation

import android.os.Build
import androidx.compose.runtime.Composable
import androidx.compose.runtime.State
import androidx.compose.ui.Modifier

@Composable
internal fun Modifier.motionShine(
    tiltState: State<TiltState>,
    parameters: MotionShineParameters
): Modifier = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
    motionShineShader(tiltState, parameters)
} else {
    motionShineFallback(tiltState, parameters)
}

@Composable
internal fun Modifier.maskedMotionShine(
    tiltState: State<TiltState>,
    parameters: MotionShineParameters,
    contentAlpha: Float = 1f
): Modifier = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
    maskedMotionShineShader(tiltState, parameters, contentAlpha)
} else {
    maskedMotionShineFallback(tiltState, parameters, contentAlpha)
}
