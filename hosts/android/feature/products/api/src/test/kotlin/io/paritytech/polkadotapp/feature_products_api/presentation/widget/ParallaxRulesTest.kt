package io.paritytech.polkadotapp.feature_products_api.presentation.widget

import android.view.Surface
import androidx.compose.ui.geometry.Offset
import io.paritytech.polkadotapp.feature_products_api.model.JsModifier
import io.paritytech.polkadotapp.feature_products_api.model.JsTiltParallax
import org.junit.Assert.assertEquals
import org.junit.Test
import kotlin.math.cos
import kotlin.math.exp
import kotlin.math.sin

class ParallaxRulesTest {
    private val tilt = JsTiltParallax(travel = 1000, maxTiltCentideg = 1000, settleMs = 0)

    private fun assertOffset(expected: Offset, actual: Offset) {
        assertEquals(expected.x, actual.x, 0.01f)
        assertEquals(expected.y, actual.y, 0.01f)
    }

    @Test
    fun `tilt moves in proportion up to its travel at the clamp, then holds`() {
        val parallax = JsModifier.Parallax(depthPermille = 1000, tilt = tilt)
        assertOffset(Offset(500f, -1000f), ParallaxRules.offset(parallax, Offset(5f, -25f), Offset.Zero))
    }

    @Test
    fun `depth scales the sum of tilt and pan, a negative depth the other way`() {
        val parallax = JsModifier.Parallax(depthPermille = 500, tilt = tilt, pan = "g")
        assertOffset(Offset(350f, 450f), ParallaxRules.offset(parallax, Offset(5f, 20f), Offset(200f, -100f)))
        assertOffset(Offset(-700f, -900f), ParallaxRules.offset(parallax.copy(depthPermille = -1000), Offset(5f, 20f), Offset(200f, -100f)))
    }

    @Test
    fun `no pan key ignores the pan, and a zero max tilt never moves`() {
        assertOffset(Offset.Zero, ParallaxRules.offset(JsModifier.Parallax(1000, tilt), Offset.Zero, Offset(200f, 200f)))
        assertOffset(Offset.Zero, ParallaxRules.offset(JsModifier.Parallax(1000, tilt.copy(maxTiltCentideg = 0)), Offset(5f, 5f), Offset.Zero))
        assertOffset(Offset(200f, 0f), ParallaxRules.offset(JsModifier.Parallax(1000, null, "g"), Offset(5f, 5f), Offset(200f, 0f)))
    }

    @Test
    fun `gravity reads the right edge and the top edge lowered as positive degrees`() {
        val g = 9.81f
        val thirty = Math.toRadians(30.0)
        assertOffset(Offset.Zero, ParallaxRules.degrees(0f, 0f, g)!!)
        assertOffset(Offset(30f, 0f), ParallaxRules.degrees((-g * sin(thirty)).toFloat(), 0f, (g * cos(thirty)).toFloat())!!)
        assertOffset(Offset(0f, 30f), ParallaxRules.degrees(0f, (-g * sin(thirty)).toFloat(), (g * cos(thirty)).toFloat())!!)
        assertEquals(null, ParallaxRules.degrees(0f, 0f, 0f))
    }

    @Test
    fun `a reading turns with the display into the screen's frame`() {
        assertEquals(1f to 2f, ParallaxRules.toScreen(1f, 2f, Surface.ROTATION_0))
        assertEquals(-2f to 1f, ParallaxRules.toScreen(1f, 2f, Surface.ROTATION_90))
        assertEquals(-1f to -2f, ParallaxRules.toScreen(1f, 2f, Surface.ROTATION_180))
        assertEquals(2f to -1f, ParallaxRules.toScreen(1f, 2f, Surface.ROTATION_270))
    }

    @Test
    fun `tilt is measured from the first attitude, kept with no settle`() {
        val rest = TiltRest()
        assertOffset(Offset.Zero, rest.tilt(Offset(10f, 20f), 0L, 0))
        assertOffset(Offset(5f, -5f), rest.tilt(Offset(15f, 15f), 5_000_000_000L, 0))
    }

    @Test
    fun `the rest attitude follows the device on its time constant`() {
        val rest = TiltRest()
        rest.tilt(Offset(0f, 0f), 0L, 100)
        val share = (1 - exp(-1.0)).toFloat()
        assertOffset(Offset(10f * (1 - share), 0f), rest.tilt(Offset(10f, 0f), 100_000_000L, 100))
    }
}
