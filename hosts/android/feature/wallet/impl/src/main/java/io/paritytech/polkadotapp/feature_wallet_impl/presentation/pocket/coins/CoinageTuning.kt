package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.sqrt

/**
 * Material settings of our own, past what the reference exports, kept apart so the knobs are one short file
 * to open while the look is tuned. `params.json` stays the file the export wrote.
 *
 * The reference is tuned for one large coin under a still light; here there are many small ones under a light
 * that moves with the hand, and the swing in brightness was the only thing the eye could see. The wear and the
 * struck figure both went under it.
 */
object CoinageTuning {
    /**
     * Light added to the whole studio, as if its grey walls were painted a lighter grey. The exported room
     * runs from five to sixteen percent of white against softboxes at seventy and more, and that seven-to-one
     * step is what a coin fell off as it tilted. This brings it to about two to one. Linear, so it is the same
     * room at every level of blur.
     */
    const val ROOM_LIFT = 0.28f

    /**
     * How much harder the struck relief slopes than the atlas says. The atlas is baked for a face hundreds of
     * points across; at sixty the figure flattened into the field.
     */
    const val RELIEF_GAIN = 1.8f

    /** A softbox of our own: a direction from the coins (+z toward the viewer), a half angle and a gain. */
    class Panel(val x: Float, val y: Float, val z: Float, val halfAngleDegrees: Float, val gain: Float)

    /**
     * One straight ahead over the key light, so the resting pose is the brightest the coin ever gets — the
     * export's own ceiling strip is as bright as its key, so without it a coin leaned back outshone one at
     * rest. One high and a fainter one low, so a coin leaned either way still finds some light. The shader
     * reads exactly three.
     */
    val panels = listOf(
        Panel(x = 0f, y = 0.15f, z = 1f, halfAngleDegrees = 28f, gain = 0.45f),
        Panel(x = 0f, y = 0.87f, z = 0.5f, halfAngleDegrees = 18f, gain = 0.3f),
        Panel(x = 0f, y = -0.87f, z = 0.5f, halfAngleDegrees = 18f, gain = 0.2f)
    )

    /** As the shader reads them: unit direction and the cosine of the half angle, then gain and padding. */
    val packedPanels: FloatArray = panels.flatMap { panel ->
        val length = maxOf(sqrt(panel.x * panel.x + panel.y * panel.y + panel.z * panel.z), 1e-6f)
        val edge = cos(panel.halfAngleDegrees * PI / 180).toFloat()

        listOf(panel.x / length, panel.y / length, panel.z / length, edge, panel.gain, 0f, 0f, 0f)
    }.toFloatArray()
}
