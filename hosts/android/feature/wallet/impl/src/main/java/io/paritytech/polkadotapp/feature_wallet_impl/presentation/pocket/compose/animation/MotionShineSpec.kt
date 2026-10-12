package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.animation

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
    }
}
