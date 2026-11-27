package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

import android.opengl.GLES20
import android.opengl.GLES30
import android.os.SystemClock
import io.paritytech.polkadotapp.common.utils.logFailure
import timber.log.Timber
import java.nio.ByteBuffer
import java.nio.ByteOrder

/**
 * Bakes [CoinageStudio] into the prefiltered cube the coin shader samples, on the thread holding the GL
 * context.
 *
 * GLES 3.0 has no compute shaders, so every face of every level is a fragment pass into a half-float render
 * target. Rendering into one is an extension in ES 3.0; nearly every GPU has it, and one without gets the same
 * studio baked on the CPU into a smaller cube.
 */
object CoinageStudioBake {
    class Cube(val texture: Int, val levels: Int)

    /** Rows per draw above which a level is split, so no single draw runs long enough to trip a GPU watchdog. */
    private const val BAND_ROWS = 128

    /** The CPU cube: small, because evaluating the studio there costs what the GPU does in a few frames. */
    private const val FALLBACK_SIZE = 128
    private const val FALLBACK_LEVELS = 6

    private const val LAMP_VECTORS = 6
    private const val MAX_LAMPS = 16

    fun bake(studio: CoinageStudio, vertexSource: String, fragmentSource: String, isCancelled: () -> Boolean): Cube? {
        val started = SystemClock.elapsedRealtime()

        if (rendersToFloat()) {
            runCatching { gpu(studio, vertexSource, fragmentSource, CoinageStudio.SIZE, CoinageStudio.LEVELS, isCancelled) }
                .logFailure("CoinageStudioBake: GPU bake failed, baking on the CPU")
                .onSuccess { cube ->
                    if (cube != null) Timber.i("CoinageStudioBake: GPU bake took ${SystemClock.elapsedRealtime() - started} ms")

                    return cube
                }
        }

        return cpu(studio).also { Timber.i("CoinageStudioBake: CPU bake took ${SystemClock.elapsedRealtime() - started} ms") }
    }

    fun rendersToFloat(): Boolean {
        val extensions = GLES20.glGetString(GLES20.GL_EXTENSIONS).orEmpty()

        return "GL_EXT_color_buffer_float" in extensions || "GL_EXT_color_buffer_half_float" in extensions
    }

    /** Exposed at any size so a test can check it against [CoinageStudio.bake] on a device. */
    fun gpu(
        studio: CoinageStudio,
        vertexSource: String,
        fragmentSource: String,
        size: Int,
        levels: Int,
        isCancelled: () -> Boolean
    ): Cube? {
        val program = CoinageGlProgram.link(vertexSource, fragmentSource)
        val texture = cube(size, levels)
        val vertexArray = IntArray(1).also { GLES30.glGenVertexArrays(1, it, 0) }[0]
        val framebuffer = IntArray(1).also { GLES20.glGenFramebuffers(1, it, 0) }[0]

        try {
            GLES20.glDisable(GLES20.GL_DEPTH_TEST)
            GLES20.glDisable(GLES20.GL_BLEND)
            GLES20.glDisable(GLES20.GL_CULL_FACE)
            GLES20.glUseProgram(program)
            GLES30.glBindVertexArray(vertexArray)
            GLES20.glBindFramebuffer(GLES20.GL_FRAMEBUFFER, framebuffer)

            val lamps = packedLamps(studio)
            GLES20.glUniform4fv(GLES20.glGetUniformLocation(program, "uLamps"), lamps.size / 4, lamps, 0)
            GLES20.glUniform1i(GLES20.glGetUniformLocation(program, "uLampCount"), studio.lamps.size)
            GLES20.glUniform2f(GLES20.glGetUniformLocation(program, "uRoom"), studio.roomLow, studio.roomHigh)

            for (level in 0 until levels) {
                val side = maxOf(size shr level, 1)
                val roughness = level.toFloat() / (levels - 1)

                GLES20.glUniform1f(GLES20.glGetUniformLocation(program, "uSize"), side.toFloat())
                GLES20.glUniform1f(GLES20.glGetUniformLocation(program, "uAlpha"), roughness * roughness)
                GLES20.glUniform1i(
                    GLES20.glGetUniformLocation(program, "uSampleCount"),
                    if (level == 0) 0 else CoinageStudio.sampleCount(roughness)
                )

                for (face in 0 until CoinageStudio.FACES) {
                    if (isCancelled()) {
                        GLES20.glDeleteTextures(1, intArrayOf(texture), 0)

                        return null
                    }

                    GLES20.glFramebufferTexture2D(
                        GLES20.GL_FRAMEBUFFER,
                        GLES20.GL_COLOR_ATTACHMENT0,
                        GLES20.GL_TEXTURE_CUBE_MAP_POSITIVE_X + face,
                        texture,
                        level
                    )
                    check(GLES20.glCheckFramebufferStatus(GLES20.GL_FRAMEBUFFER) == GLES20.GL_FRAMEBUFFER_COMPLETE) {
                        "studio level $level face $face is not renderable"
                    }
                    GLES20.glUniform1i(GLES20.glGetUniformLocation(program, "uFace"), face)
                    GLES20.glViewport(0, 0, side, side)
                    GLES20.glEnable(GLES20.GL_SCISSOR_TEST)

                    for (band in 0 until side step BAND_ROWS) {
                        GLES20.glScissor(0, band, side, minOf(BAND_ROWS, side - band))
                        GLES30.glDrawArrays(GLES20.GL_TRIANGLES, 0, 3)
                        GLES20.glFlush()
                    }

                    GLES20.glDisable(GLES20.GL_SCISSOR_TEST)
                }

                // Each level waited for in turn, as on iOS: the first two are most of the work.
                GLES20.glFinish()
            }
        } catch (failure: RuntimeException) {
            GLES20.glDeleteTextures(1, intArrayOf(texture), 0)
            throw failure
        } finally {
            GLES20.glBindFramebuffer(GLES20.GL_FRAMEBUFFER, 0)
            GLES20.glDeleteFramebuffers(1, intArrayOf(framebuffer), 0)
            GLES30.glDeleteVertexArrays(1, intArrayOf(vertexArray), 0)
            GLES20.glDeleteProgram(program)
        }

        return Cube(texture, levels)
    }

    private fun cpu(studio: CoinageStudio): Cube {
        val texture = cube(FALLBACK_SIZE, FALLBACK_LEVELS)

        studio.bake(FALLBACK_SIZE, FALLBACK_LEVELS).forEachIndexed { level, faces ->
            val side = maxOf(FALLBACK_SIZE shr level, 1)

            faces.forEachIndexed { face, rgb ->
                val rgba = ByteBuffer.allocateDirect(side * side * 4 * Float.SIZE_BYTES).order(ByteOrder.nativeOrder())
                val floats = rgba.asFloatBuffer()

                for (texel in 0 until side * side) {
                    floats.put(rgb[texel * 3]).put(rgb[texel * 3 + 1]).put(rgb[texel * 3 + 2]).put(1f)
                }

                GLES20.glTexSubImage2D(
                    GLES20.GL_TEXTURE_CUBE_MAP_POSITIVE_X + face, level, 0, 0, side, side,
                    GLES20.GL_RGBA, GLES20.GL_FLOAT, rgba
                )
            }
        }

        return Cube(texture, FALLBACK_LEVELS)
    }

    private fun cube(size: Int, levels: Int): Int {
        val name = IntArray(1).also { GLES20.glGenTextures(1, it, 0) }[0]

        GLES20.glBindTexture(GLES20.GL_TEXTURE_CUBE_MAP, name)
        GLES30.glTexStorage2D(GLES20.GL_TEXTURE_CUBE_MAP, levels, GLES30.GL_RGBA16F, size, size)
        GLES20.glTexParameteri(GLES20.GL_TEXTURE_CUBE_MAP, GLES20.GL_TEXTURE_MIN_FILTER, GLES20.GL_LINEAR_MIPMAP_LINEAR)
        GLES20.glTexParameteri(GLES20.GL_TEXTURE_CUBE_MAP, GLES20.GL_TEXTURE_MAG_FILTER, GLES20.GL_LINEAR)
        GLES20.glTexParameteri(GLES20.GL_TEXTURE_CUBE_MAP, GLES20.GL_TEXTURE_WRAP_S, GLES20.GL_CLAMP_TO_EDGE)
        GLES20.glTexParameteri(GLES20.GL_TEXTURE_CUBE_MAP, GLES20.GL_TEXTURE_WRAP_T, GLES20.GL_CLAMP_TO_EDGE)

        return name
    }

    /** Six `vec4` a lamp, as `studio.frag` reads them. */
    private fun packedLamps(studio: CoinageStudio): FloatArray {
        check(studio.lamps.size <= MAX_LAMPS) { "the bake shader holds at most $MAX_LAMPS lamps" }

        return studio.lamps.flatMap { lamp ->
            listOf(
                lamp.center.x, lamp.center.y, lamp.center.z, 0f,
                lamp.uAxis.x, lamp.uAxis.y, lamp.uAxis.z, 0f,
                lamp.vAxis.x, lamp.vAxis.y, lamp.vAxis.z, 0f,
                lamp.normal.x, lamp.normal.y, lamp.normal.z, 0f,
                lamp.tint.x, lamp.tint.y, lamp.tint.z, 0f,
                lamp.halfU, lamp.halfV, lamp.intensity, lamp.roomHere
            )
        }.toFloatArray().copyOf(MAX_LAMPS * LAMP_VECTORS * 4)
    }
}
