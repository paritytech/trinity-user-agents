package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.float
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.sin
import kotlin.math.sqrt

class CoinageStudioTest {
    private val paramsJson = File("src/main/assets/coinage/params.json").readText()
    private val studio = CoinageAssetStore.studio(paramsJson)

    /** Every shipped lamp is the rectangle its azimuth, elevation, distance and size describe. */
    @Test
    fun `the shipped lamps follow from their placement`() {
        val lights = Json.parseToJsonElement(paramsJson).jsonObject["lights"]!!.jsonArray

        assertEquals(15, studio.lamps.size)

        lights.forEachIndexed { index, light ->
            fun value(key: String) = light.jsonObject[key]!!.jsonPrimitive.float
            val az = Math.toRadians(value("az").toDouble())
            val el = Math.toRadians(value("el").toDouble())
            val d = CoinageStudio.Vec((sin(az) * cos(el)).toFloat(), sin(el).toFloat(), (cos(az) * cos(el)).toFloat())
            val normal = d * -1f
            val up = if (abs(d.y) > 0.95f) CoinageStudio.Vec(0f, 0f, -1f) else CoinageStudio.Vec(0f, 1f, 0f)
            val uAxis = (up cross normal).normalized()
            val lamp = studio.lamps[index]

            near("center $index", d * value("dist"), lamp.center)
            near("normal $index", normal, lamp.normal)
            near("u axis $index", uAxis, lamp.uAxis)
            near("v axis $index", normal cross uAxis, lamp.vAxis)
            assertEquals(value("width") / 2, lamp.halfU, 1e-4f)
            assertEquals(value("height") / 2, lamp.halfV, 1e-4f)

            val room = studio.roomLow + (studio.roomHigh - studio.roomLow) * smoothstep(-0.35f, 0.85f, sin(el).toFloat())
            assertEquals("room at lamp $index", room, lamp.roomHere, 1e-4f)
        }
    }

    /** A coin at rest mirrors the small front panel, at its full brightness. */
    @Test
    fun `a coin at rest mirrors the front lamp`() {
        val head = studio.radiance(CoinageStudio.Vec(0f, 0f, 1f))
        val front = studio.lamps.last()

        assertEquals(front.intensity + front.roomHere, head.x, 1e-4f)
    }

    @Test
    fun `away from every lamp is the room`() {
        val floor = studio.radiance(CoinageStudio.Vec(0f, -1f, 0f))

        assertEquals(studio.roomLow, floor.x, 1e-6f)
    }

    /** Each rougher level averages the same studio over a wider lobe: about the same mean, less spread. */
    @Test
    fun `rougher levels keep the mean and lose the spread`() {
        val cube = studio.bake(size = 16, levels = 5)
        val stats = cube.map { faces -> faces.flatMap { face -> face.asList() }.let { it.average() to spread(it) } }

        for (level in 1 until stats.size) {
            val (mean, deviation) = stats[level]
            val (previousMean, previousDeviation) = stats[level - 1]

            assertTrue("level $level spreads less than ${level - 1}", deviation < previousDeviation)
            assertEquals("level $level keeps the mean", previousMean, mean, previousMean * 0.25)
        }
    }

    @Test
    fun `the radical inverse mirrors the bits`() {
        assertEquals(0f, CoinageStudio.radicalInverse(0))
        assertEquals(0.5f, CoinageStudio.radicalInverse(1))
        assertEquals(0.25f, CoinageStudio.radicalInverse(2))
        assertEquals(0.75f, CoinageStudio.radicalInverse(3))
    }

    private fun spread(values: List<Float>): Double {
        val mean = values.average()

        return sqrt(values.sumOf { (it - mean) * (it - mean) } / values.size)
    }

    private fun near(what: String, expected: CoinageStudio.Vec, actual: CoinageStudio.Vec) {
        assertEquals("$what x", expected.x, actual.x, 1e-4f)
        assertEquals("$what y", expected.y, actual.y, 1e-4f)
        assertEquals("$what z", expected.z, actual.z, 1e-4f)
    }

    private fun smoothstep(e0: Float, e1: Float, x: Float): Float {
        val t = ((x - e0) / (e1 - e0)).coerceIn(0f, 1f)

        return t * t * (3f - 2f * t)
    }
}
