package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

import android.opengl.EGL14
import android.opengl.EGLConfig
import android.opengl.EGLContext
import android.opengl.EGLDisplay
import android.opengl.EGLExt
import android.opengl.EGLSurface
import android.opengl.GLES20
import android.opengl.GLES30
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.After
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.math.abs

/** The GPU bake against the CPU evaluation of the same lamps, on every face of every level of a small cube. */
@RunWith(AndroidJUnit4::class)
class CoinageStudioBakeTest {
    private lateinit var display: EGLDisplay
    private lateinit var context: EGLContext
    private lateinit var surface: EGLSurface

    @Before
    fun setUp() {
        display = EGL14.eglGetDisplay(EGL14.EGL_DEFAULT_DISPLAY)
        EGL14.eglInitialize(display, IntArray(2), 0, IntArray(2), 1)

        val configs = arrayOfNulls<EGLConfig>(1)
        EGL14.eglChooseConfig(
            display,
            intArrayOf(
                EGL14.EGL_RENDERABLE_TYPE, EGLExt.EGL_OPENGL_ES3_BIT_KHR,
                EGL14.EGL_SURFACE_TYPE, EGL14.EGL_PBUFFER_BIT,
                EGL14.EGL_NONE
            ),
            0, configs, 0, 1, IntArray(1), 0
        )
        context = EGL14.eglCreateContext(
            display, configs[0], EGL14.EGL_NO_CONTEXT, intArrayOf(EGL14.EGL_CONTEXT_CLIENT_VERSION, 3, EGL14.EGL_NONE), 0
        )
        surface = EGL14.eglCreatePbufferSurface(
            display, configs[0], intArrayOf(EGL14.EGL_WIDTH, 1, EGL14.EGL_HEIGHT, 1, EGL14.EGL_NONE), 0
        )
        EGL14.eglMakeCurrent(display, surface, surface, context)
    }

    @After
    fun tearDown() {
        EGL14.eglMakeCurrent(display, EGL14.EGL_NO_SURFACE, EGL14.EGL_NO_SURFACE, EGL14.EGL_NO_CONTEXT)
        EGL14.eglDestroySurface(display, surface)
        EGL14.eglDestroyContext(display, context)
        EGL14.eglTerminate(display)
    }

    @Test
    fun theGpuBakeMatchesTheCpuEvaluation() {
        assumeTrue("needs float render targets", CoinageStudioBake.rendersToFloat())

        val assets = InstrumentationRegistry.getInstrumentation().targetContext.assets
        fun text(name: String) = assets.open("coinage/$name").bufferedReader().use { it.readText() }

        val studio = CoinageAssetStore.studio(text("params.json"))
        val cube = checkNotNull(
            CoinageStudioBake.gpu(studio, text("studio.vert"), text("studio.frag"), SIZE, LEVELS, isCancelled = { false })
        )
        val expected = studio.bake(SIZE, LEVELS)

        for (level in 0 until LEVELS) {
            val side = maxOf(SIZE shr level, 1)
            var off = 0

            for (face in 0 until CoinageStudio.FACES) {
                val gpu = readBack(cube.texture, face, level, side)
                val cpu = expected[level][face]

                for (texel in 0 until side * side) {
                    for (channel in 0..2) {
                        val want = cpu[texel * 3 + channel]
                        val got = gpu[texel * 4 + channel]

                        // Half floats, and a texel on a lamp's edge may land either side of it.
                        if (abs(got - want) > 0.01f + 0.01f * want) off++
                    }
                }
            }

            val share = off.toFloat() / (CoinageStudio.FACES * side * side * 3)
            assertTrue("level $level: ${share * 100}% of channels differ", share < 0.02f)
        }
    }

    private fun readBack(texture: Int, face: Int, level: Int, side: Int): FloatArray {
        val framebuffer = IntArray(1).also { GLES20.glGenFramebuffers(1, it, 0) }[0]
        val pixels = ByteBuffer.allocateDirect(side * side * 4 * 4).order(ByteOrder.nativeOrder())

        GLES20.glBindFramebuffer(GLES20.GL_FRAMEBUFFER, framebuffer)
        GLES20.glFramebufferTexture2D(
            GLES20.GL_FRAMEBUFFER, GLES20.GL_COLOR_ATTACHMENT0, GLES20.GL_TEXTURE_CUBE_MAP_POSITIVE_X + face, texture, level
        )
        GLES30.glReadPixels(0, 0, side, side, GLES20.GL_RGBA, GLES20.GL_FLOAT, pixels)
        GLES20.glBindFramebuffer(GLES20.GL_FRAMEBUFFER, 0)
        GLES20.glDeleteFramebuffers(1, intArrayOf(framebuffer), 0)

        return FloatArray(side * side * 4).also { pixels.asFloatBuffer().get(it) }
    }

    private companion object {
        const val SIZE = 32
        const val LEVELS = 6
    }
}
