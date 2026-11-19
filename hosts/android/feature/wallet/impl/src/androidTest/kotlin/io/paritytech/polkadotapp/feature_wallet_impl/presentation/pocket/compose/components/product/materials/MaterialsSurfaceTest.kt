package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.product.materials

import android.graphics.Bitmap
import android.graphics.Color
import android.graphics.PixelFormat
import android.graphics.SurfaceTexture
import android.media.ImageReader
import android.opengl.GLES30
import android.os.Handler
import android.os.HandlerThread
import android.view.Surface
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
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
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

// Shaders in the shape the core writes them for OpenGL ES (shader.rs), drawn on the device's own GPU.
@RunWith(AndroidJUnit4::class)
class MaterialsSurfaceTest {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val readerThread = HandlerThread("materials-test-reader").apply { start() }
    private var reader = ImageReader.newInstance(SIDE, SIDE, PixelFormat.RGBA_8888, 2)
    private var surface: MaterialsSurface? = null

    @Before
    fun startGl() = MaterialsGl.start(context)

    @After
    fun tearDown() {
        surface?.release()
        onGl { }
        reader.close()
        readerThread.quitSafely()
    }

    @Test
    fun compilesOnTheGlThread() {
        assertTrue(MaterialsGl.program(material(1), shader(IDENTITY)).isFailure)

        val compiledOn = onGl { Thread.currentThread().name to MaterialsGl.program(material(1), shader(IDENTITY)).isSuccess }

        assertEquals("materials-gl" to true, compiledOn)
    }

    @Test
    fun identityReproducesBase() {
        val drawn = draw(listOf(layer(1, IDENTITY)))

        assertEquals(listOf(RED, BLUE), listOf(drawn.top, drawn.bottom))
    }

    @Test
    fun invertStaysInsideNode() {
        val drawn = draw(listOf(layer(2, INVERT)))

        assertEquals(listOf(CYAN, YELLOW), listOf(drawn.top, drawn.bottom))
    }

    // Each layer reads the one before it, so the same layers in another order draw something else.
    @Test
    fun layerOrderMatters() {
        assumeTrue("needs EXT_color_buffer_float", onGl { MaterialsGl.floatTargets })

        val tintThenInvert = draw(listOf(layer(3, HALF_RED), layer(2, INVERT)))
        reopen()
        val invertThenTint = draw(listOf(layer(2, INVERT), layer(3, HALF_RED)))

        assertEquals(listOf(128, 255, 255), tintThenInvert.top.channels().map { it.roundToNearest(128) })
        assertEquals(CYAN, invertThenTint.top)
        assertNotEquals(tintThenInvert.top, invertThenTint.top)
    }

    // Layers between the base and the window draw into float targets row for row; a flip between
    // them would turn the card upside down.
    @Test
    fun baseStaysTopUp() {
        assumeTrue("needs EXT_color_buffer_float", onGl { MaterialsGl.floatTargets })

        val drawn = draw(listOf(layer(1, IDENTITY), layer(4, IDENTITY_TOO)))

        assertEquals(listOf(RED, BLUE), listOf(drawn.top, drawn.bottom))
    }

    @Test
    fun syntaxErrorFailsOnce() {
        val failures = AtomicInteger()
        val failed = CountDownLatch(1)
        val broken = MaterialsSurface(
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
        onGl { }
        assertEquals(listOf(1, 0), listOf(failures.get(), broken.framesDrawn))
    }

    // A Surface released before the GL thread reaches it must fail its node, not end the process.
    @Test
    fun releasedSurfaceFailsTheNode() {
        val failed = CountDownLatch(1)
        val texture = SurfaceTexture(false).apply { setDefaultBufferSize(SIDE, SIDE) }
        val released = Surface(texture).apply { release() }

        surface = MaterialsSurface(onFirstFrame = {}, onFailure = { _, _ -> failed.countDown() }, onTooSlow = {})
            .apply { attach(released, SIDE, SIDE) }

        assertTrue(failed.await(TIMEOUT_SECONDS, TimeUnit.SECONDS))
        assertEquals("the GL thread still serves", "materials-gl", onGl { Thread.currentThread().name })
        texture.release()
    }

    // A fresh process has nothing linked in memory; it reads the program the last one kept.
    @Test
    fun linkedProgramReloadsFromDisk() {
        val directory = File(context.cacheDir, "materials-test-${System.nanoTime()}")
        val guard = CompileGuard(context.getSharedPreferences("materials-test-guard", 0))

        val sources = onGl {
            MaterialsGl.makeCurrent(null)
            if (!driverReloadsItsOwnBinaries()) return@onGl null
            List(2) {
                GlPrograms(directory, guard, "test", keepsBinaries = true).program(material(6), shader(IDENTITY))
            }.map { it.getOrThrow().fromDisk }
        }
        directory.deleteRecursively()

        assumeTrue("the driver does not reload program binaries", sources != null)
        assertEquals(listOf(false, true), sources)
    }

    // The reload is measured straight on GL: some drivers (SwiftShader) list binary formats yet refuse
    // every binary they hand out, and there the cache can only recompile.
    private fun driverReloadsItsOwnBinaries(): Boolean {
        val formats = IntArray(1)
        GLES30.glGetIntegerv(GLES30.GL_NUM_PROGRAM_BINARY_FORMATS, formats, 0)
        if (formats[0] == 0) return false
        val linked = MaterialsGl.program(material(8), shader(INVERT)).getOrThrow()
        val length = IntArray(1)
        GLES30.glGetProgramiv(linked, GLES30.GL_PROGRAM_BINARY_LENGTH, length, 0)
        if (length[0] == 0) return false
        val binary = ByteBuffer.allocateDirect(length[0]).order(ByteOrder.nativeOrder())
        val format = IntArray(1)
        GLES30.glGetProgramBinary(linked, length[0], length, 0, format, 0, binary)
        val reloaded = GLES30.glCreateProgram()
        GLES30.glProgramBinary(reloaded, format[0], binary, length[0])
        val status = IntArray(1)
        GLES30.glGetProgramiv(reloaded, GLES30.GL_LINK_STATUS, status, 0)
        GLES30.glDeleteProgram(reloaded)
        return status[0] != 0
    }

    private class Drawn(val top: Int, val bottom: Int)

    private fun draw(layers: List<ReadyLayer>): Drawn {
        val firstFrame = CountDownLatch(1)
        val shown = CountDownLatch(1)
        var failure: MaterialFailure? = null
        reader.setOnImageAvailableListener({ shown.countDown() }, Handler(readerThread.looper))
        surface = MaterialsSurface(
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

    private fun reopen() {
        surface?.release()
        surface = null
        onGl { }
        reader.close()
        reader = ImageReader.newInstance(SIDE, SIDE, PixelFormat.RGBA_8888, 2)
    }

    private fun base(): Bitmap = Bitmap.createBitmap(SIDE, SIDE, Bitmap.Config.ARGB_8888).apply {
        for (y in 0 until SIDE) for (x in 0 until SIDE) setPixel(x, y, if (y < SIDE / 2) Color.RED else Color.BLUE)
    }

    private companion object {
        const val SIDE = 16
        const val NOISE_BYTES = 1024 * 1024
        const val IDENTITY = "vec4 material(vec2 uv){return texture(r_input, uv);}"
        const val IDENTITY_TOO = "vec4 material(vec2 uv){vec4 c=texture(r_input, uv);return c;}"
        const val INVERT = "vec4 material(vec2 uv){vec4 c=texture(r_input, uv);return vec4(1.0-c.rgb,c.a);}"
        const val HALF_RED = "vec4 material(vec2 uv){vec4 c=texture(r_input, uv);return vec4(c.r*0.5,c.g,c.b,c.a);}"
        val RED = Color.rgb(255, 0, 0)
        val BLUE = Color.rgb(0, 0, 255)
        val CYAN = Color.rgb(0, 255, 255)
        val YELLOW = Color.rgb(255, 255, 0)
    }
}

internal const val TIMEOUT_SECONDS = 10L

private fun Int.channels() = listOf(Color.red(this), Color.green(this), Color.blue(this))

// 0.5 lands on either side of 127.5 depending on the GPU's rounding.
private fun Int.roundToNearest(target: Int) = if (kotlin.math.abs(this - target) <= 1) target else this

internal fun <T> onGl(block: () -> T): T {
    val done = CountDownLatch(1)
    var result: Result<T>? = null
    MaterialsGl.handler.post {
        result = runCatching(block)
        done.countDown()
    }
    assertTrue(done.await(TIMEOUT_SECONDS, TimeUnit.SECONDS))
    return checkNotNull(result).getOrThrow()
}

internal fun material(id: Int) = ByteArray(32) { id.toByte() }

internal fun layer(id: Int, body: String, readsTime: Boolean = false) = ReadyLayer(material(id), shader(body, readsTime), emptyList())

internal fun shader(body: String, readsTime: Boolean = false) = MaterialShader(
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
    readsTime = readsTime,
    readsMotion = false,
)
