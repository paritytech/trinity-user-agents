package io.paritytech.polkadotapp.feature_products_api.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
data class JsCurve(val x1: Int, val y1: Int, val x2: Int, val y2: Int)

@Serializable
sealed interface JsMotion {
    @Serializable
    @SerialName("timed")
    data class Timed(val ms: Long, val curve: JsCurve) : JsMotion

    @Serializable
    @SerialName("spring")
    data class Spring(val omegaCenti: Int, val dampingCenti: Int) : JsMotion
}

@Serializable
data class JsPose(val opacityPermille: Int, val scalePermille: Int, val x: Int, val y: Int, val rotationCentideg: Int)

@Serializable
data class JsEnter(val from: JsPose, val motion: JsMotion, val delayMs: Int)

@Serializable
enum class JsAnimatedProperty { OPACITY, SCALE, OFFSET_X, OFFSET_Y, ROTATION }

@Serializable
data class JsKeyframe(val atMs: Long, val value: Int, val curve: JsCurve)

@Serializable
sealed interface JsTrackPlay {
    @Serializable
    @SerialName("keyframes")
    data class Keyframes(val frames: List<JsKeyframe>, val repeat: Int, val alternate: Boolean) : JsTrackPlay

    @Serializable
    @SerialName("follow")
    data class Follow(val target: Int, val motion: JsMotion) : JsTrackPlay
}

@Serializable
data class JsTrack(val property: JsAnimatedProperty, val play: JsTrackPlay)
