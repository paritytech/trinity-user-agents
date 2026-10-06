package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import kotlin.math.abs
import kotlin.math.sqrt

/**
 * Keeps the door shut on the mistake §12 of the port brief warns about.
 *
 * The relief atlas is data, not an image: its RGB is a struck normal and its alpha is a height. Anything
 * that decodes it as a picture is entitled to premultiply the alpha, and would hand back every normal scaled
 * by its own height — a flat, quietly wrong relief rather than anything that fails. It is vendored as raw
 * RGBA bytes for exactly that reason, and this reads the shipped file to prove the normals still have unit
 * length.
 *
 * Reads the asset off disk rather than through an `AssetManager`, which a JVM test has no access to. Gradle
 * runs unit tests with the module directory as the working directory, so the path is stable.
 */
class CoinageReliefAtlasTest {
    @Test
    fun `the shipped atlas holds unit normals, not premultiplied ones`() {
        val bytes = atlas.readBytes()

        assertEquals("atlas size", SIZE * SIZE * CHANNELS, bytes.size)

        var checked = 0
        var worst = 0.0

        // Every hundredth texel: four million is more than the point needs, and a premultiplied decode would
        // shorten all of them rather than a few.
        var offset = 0
        while (offset < bytes.size) {
            val x = decode(bytes[offset])
            val y = decode(bytes[offset + 1])
            val z = decode(bytes[offset + 2])
            val length = sqrt(x * x + y * y + z * z)

            worst = maxOf(worst, abs(length - 1.0))
            checked++
            offset += CHANNELS * STRIDE
        }

        assertTrue("nothing was checked", checked > 1000)
        assertTrue(
            "normals are not unit length: worst error $worst over $checked texels",
            worst <= TOLERANCE
        )
    }

    @Test
    fun `the height channel is not constant, which a dropped alpha would make it`() {
        val bytes = atlas.readBytes()
        val heights = (3 until bytes.size step CHANNELS * STRIDE).map { bytes[it].toInt() and 0xFF }

        assertTrue("the height channel carries no relief", heights.distinct().size > 2)
    }

    private fun decode(byte: Byte) = (byte.toInt() and 0xFF) / 255.0 * 2 - 1

    private val atlas: File
        get() {
            val file = File("src/main/assets/coinage/relief/cash-relief.bin")

            assertTrue("missing ${file.absolutePath}", file.exists())

            return file
        }

    private companion object {
        const val SIZE = 1024
        const val CHANNELS = 4
        const val STRIDE = 97

        /**
         * The normals are quantised to eight bits a channel, so a whole step is about 0.008 and the worst
         * rounding of a unit vector lands a little under twice that. A premultiplied decode would scale a
         * normal by its own height and miss by an order of magnitude more.
         */
        const val TOLERANCE = 0.02
    }
}
