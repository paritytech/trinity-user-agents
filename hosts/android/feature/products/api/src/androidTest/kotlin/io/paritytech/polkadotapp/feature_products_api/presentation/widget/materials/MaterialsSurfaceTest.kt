package io.paritytech.polkadotapp.feature_products_api.presentation.widget.materials

import android.graphics.Bitmap
import android.graphics.Color
import android.graphics.PixelFormat
import android.graphics.SurfaceTexture
import android.media.ImageReader
import android.os.Handler
import android.os.HandlerThread
import android.view.Surface
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.truapi.MaterialFailure
import uniffi.truapi.MaterialShader
import uniffi.truapi.ReadyLayer
import uniffi.truapi.Stage
import uniffi.truapi.Texture
import uniffi.truapi.TextureRole
import uniffi.truapi.Uniform
import uniffi.truapi.UniformKind
import uniffi.truapi.UniformRole
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

// Shaders in the shape the core writes them for OpenGL ES (shader.rs), drawn on the device's own GPU.
@RunWith(AndroidJUnit4::class)
class MaterialsSurfaceTest {
    private val gl = MaterialsGl(InstrumentationRegistry.getInstrumentation().targetContext)
    private val readerThread = HandlerThread("materials-test-reader").apply { start() }
    private var reader = ImageReader.newInstance(SIDE, SIDE, PixelFormat.RGBA_8888, 2)
    private var surface: MaterialsSurface? = null

    @After
    fun tearDown() {
        surface?.release()
        gl.onGl { }
        reader.close()
        readerThread.quitSafely()
    }

    @Test
    fun identityReproducesBase() {
        val drawn = draw(listOf(layer(1, IDENTITY)))

        assertEquals(listOf(RED, BLUE), listOf(drawn.top, drawn.bottom))
    }

    @Test
    fun syntaxErrorFailsOnce() {
        val failures = AtomicInteger()
        val failed = CountDownLatch(1)
        val broken = MaterialsSurface(
            gl = gl,
            onFirstFrame = {},
            onFailure = { _, _ ->
                failures.incrementAndGet()
                failed.countDown()
            },
            onTooSlow = {},
        ).also { surface = it }

        broken.attach(reader.surface, SIDE, SIDE)
        broken.setBase(base())
        broken.setLayers(listOf(layer(5, "vec4 material(vec2 uv){return oops;}")), emptyMap(), ByteArray(NOISE_BYTES))
        broken.setBase(base())

        assertTrue(failed.await(TIMEOUT_SECONDS, TimeUnit.SECONDS))
        gl.onGl { }
        assertEquals(1, failures.get())
    }

    // A Surface released before the GL thread reaches it must fail its node, not end the process.
    @Test
    fun releasedSurfaceFailsTheNode() {
        val failed = CountDownLatch(1)
        val texture = SurfaceTexture(false).apply { setDefaultBufferSize(SIDE, SIDE) }
        val released = Surface(texture).apply { release() }

        surface = MaterialsSurface(gl = gl, onFirstFrame = {}, onFailure = { _, _ -> failed.countDown() }, onTooSlow = {})
            .apply { attach(released, SIDE, SIDE) }

        assertTrue(failed.await(TIMEOUT_SECONDS, TimeUnit.SECONDS))
        assertEquals("the GL thread still serves", "materials-gl", gl.onGl { Thread.currentThread().name })
        texture.release()
    }

    private class Drawn(val top: Int, val bottom: Int)

    private fun draw(layers: List<ReadyLayer>): Drawn {
        val firstFrame = CountDownLatch(1)
        val shown = CountDownLatch(1)
        var failure: MaterialFailure? = null
        reader.setOnImageAvailableListener({ shown.countDown() }, Handler(readerThread.looper))
        surface = MaterialsSurface(
            gl = gl,
            onFirstFrame = { firstFrame.countDown() },
            onFailure = { _, reason ->
                failure = reason
                firstFrame.countDown()
            },
            onTooSlow = {},
        ).apply {
            attach(reader.surface, SIDE, SIDE)
            setBase(base())
            setLayers(layers, emptyMap(), ByteArray(NOISE_BYTES))
        }
        assertTrue(firstFrame.await(TIMEOUT_SECONDS, TimeUnit.SECONDS))
        assertEquals(null, failure)
        assertTrue(shown.await(TIMEOUT_SECONDS, TimeUnit.SECONDS))

        val image = reader.acquireLatestImage()
        try {
            val plane = image.planes[0]
            val bytes = plane.buffer
            fun pixel(row: Int): Int {
                val at = row * plane.rowStride + (SIDE / 2) * plane.pixelStride
                return Color.rgb(bytes.get(at).toInt() and 0xff, bytes.get(at + 1).toInt() and 0xff, bytes.get(at + 2).toInt() and 0xff)
            }
            return Drawn(top = pixel(1), bottom = pixel(SIDE - 2))
        } finally {
            image.close()
        }
    }

    private fun base(): Bitmap = Bitmap.createBitmap(SIDE, SIDE, Bitmap.Config.ARGB_8888).apply {
        for (y in 0 until SIDE) for (x in 0 until SIDE) setPixel(x, y, if (y < SIDE / 2) Color.RED else Color.BLUE)
    }

    private companion object {
        const val SIDE = 16
        const val NOISE_BYTES = 1024 * 1024
        const val IDENTITY = "vec4 material(vec2 uv){return texture(r_input, uv);}"
        val RED = Color.rgb(255, 0, 0)
        val BLUE = Color.rgb(0, 0, 255)
    }
}

private const val TIMEOUT_SECONDS = 10L

private fun <T> MaterialsGl.onGl(block: () -> T): T {
    val done = CountDownLatch(1)
    var result: Result<T>? = null
    handler.post {
        result = runCatching(block)
        done.countDown()
    }
    assertTrue(done.await(TIMEOUT_SECONDS, TimeUnit.SECONDS))
    return checkNotNull(result).getOrThrow()
}

private fun material(id: Int) = ByteArray(32) { id.toByte() }

private fun layer(id: Int, body: String) = ReadyLayer(material(id), shader(body), emptyList())

private fun shader(body: String) = MaterialShader(
    vertex = "#version 300 es\nin vec2 position;out vec2 uv;\nuniform float _shaderlab_target_flip;\n" +
        "void main(){uv=vec2(position.x*.5+.5,.5-position.y*.5);" +
        "gl_Position=vec4(position.x,position.y*_shaderlab_target_flip,0.,1.);}",
    fragment = "#version 300 es\nprecision highp float;precision highp sampler2D;in vec2 uv;out vec4 result;\n" +
        "uniform float sl_time;uniform vec2 sl_tilt;uniform vec2 sl_resolution;\n" +
        "uniform vec3 sl_orientation;uniform mat3 sl_rotation;\n" +
        "uniform highp sampler2D r_base;uniform highp sampler2D r_input;\nuniform highp sampler2D r_noise;\n" +
        "#line 1\n$body\n" +
        "void main(){vec2 uv=vec2(gl_FragCoord.x, gl_FrontFacing ? sl_resolution.y-gl_FragCoord.y : gl_FragCoord.y)" +
        "/sl_resolution;result=material(uv);}",
    uniforms = listOf(
        Uniform("_shaderlab_target_flip", UniformKind.FLOAT, Stage.VERTEX, 0u, UniformRole.TargetFlip),
        Uniform("sl_time", UniformKind.FLOAT, Stage.FRAGMENT, 0u, UniformRole.Time),
        Uniform("sl_resolution", UniformKind.VEC2, Stage.FRAGMENT, 8u, UniformRole.Resolution),
    ),
    vertexBlockSize = 16u,
    fragmentBlockSize = 16u,
    textures = listOf(Texture("r_base", TextureRole.Base), Texture("r_input", TextureRole.Input), Texture("r_noise", TextureRole.Noise)),
    readsTime = false,
    readsMotion = false,
)
