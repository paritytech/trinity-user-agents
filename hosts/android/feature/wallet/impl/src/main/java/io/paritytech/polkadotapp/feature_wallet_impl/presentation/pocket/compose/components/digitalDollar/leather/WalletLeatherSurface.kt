package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar.leather

import android.content.res.AssetManager
import android.graphics.Bitmap
import android.opengl.EGL14
import android.opengl.EGLConfig
import android.opengl.EGLContext
import android.opengl.EGLDisplay
import android.opengl.EGLExt
import android.opengl.EGLSurface
import android.opengl.GLES30
import android.os.Handler
import android.os.HandlerThread
import android.os.Looper
import android.view.Choreographer
import android.view.Surface
import androidx.compose.foundation.AndroidEmbeddedExternalSurface
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Box
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalInspectionMode
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.animation.LocalCardTilt
import timber.log.Timber
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.CountDownLatch
import kotlin.math.abs
import kotlin.math.exp

@Composable
internal fun WalletLeatherSurface(modifier: Modifier = Modifier) {
    val assets = LocalContext.current.assets
    val tilt = LocalCardTilt.current
    val light = remember { WalletLight() }

    LaunchedEffect(tilt, light) {
        // The shared tilt's y grows as the phone stands up; the shader's grows as it lays flatter.
        snapshotFlow { tilt.value }.collect { light.aim(handX = it.x, handY = -it.y) }
    }

    Box(modifier = modifier) {
        // The last lit frame, so a not-yet-drawn surface or the shared-element overlay never shows the page through.
        WalletLeatherSnapshot.latest?.let { frame ->
            Image(
                modifier = Modifier.matchParentSize(),
                bitmap = frame,
                contentDescription = null,
                contentScale = ContentScale.FillBounds
            )
        }

        if (!LocalInspectionMode.current) {
            AndroidEmbeddedExternalSurface(modifier = Modifier.matchParentSize(), isOpaque = false) {
                onSurface { surface, width, height ->
                    val loop = WalletLeatherLoop(surface, assets, light)

                    loop.start(width, height)
                    surface.onChanged { newWidth, newHeight -> loop.resize(newWidth, newHeight) }
                    surface.onDestroyed { loop.stop() }
                }
            }
        }
    }
}

/**
 * The most recent frame a wallet surface drew once its light came to rest, shared by every card on screen.
 * Read back once per settle, not per frame, so it costs nothing while the light moves.
 */
internal object WalletLeatherSnapshot {
    var latest by mutableStateOf<ImageBitmap?>(null)
        private set

    private val main = Handler(Looper.getMainLooper())

    /** On the GL thread, right after a frame that will stay on screen. */
    fun capture(width: Int, height: Int) {
        if (width <= 0 || height <= 0) return

        val pixels = ByteBuffer.allocateDirect(width * height * 4).order(ByteOrder.nativeOrder())

        GLES30.glReadPixels(0, 0, width, height, GLES30.GL_RGBA, GLES30.GL_UNSIGNED_BYTE, pixels)

        // GL reads bottom row first; a bitmap stores top row first.
        val flipped = ByteBuffer.allocateDirect(width * height * 4)
        val row = width * 4

        for (y in height - 1 downTo 0) {
            pixels.limit((y + 1) * row).position(y * row)
            flipped.put(pixels)
        }

        flipped.rewind()

        // Premultiplied RGBA, which is what an ARGB_8888 bitmap holds in memory.
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        bitmap.copyPixelsFromBuffer(flipped)

        main.post { latest = bitmap.asImageBitmap() }
    }
}

/**
 * Where the light is heading and where it has got to, in the shader's tilt units. Written from the main
 * thread, eased on the render thread.
 */
internal class WalletLight {
    @Volatile
    var targetX = REST_X
        private set

    @Volatile
    var targetY = REST_Y
        private set

    @Volatile
    var onAim: (() -> Unit)? = null

    private var wokenX = REST_X
    private var wokenY = REST_Y

    fun aim(handX: Float, handY: Float) {
        targetX = REST_X + AMPLITUDE * handX.coerceIn(-1f, 1f)
        targetY = REST_Y + AMPLITUDE * handY.coerceIn(-1f, 1f)

        // Sensor noise on a phone lying still moves the target by more than a drawable step at every reading;
        // waking the render thread for it kept the card's GL thread busy for nothing.
        if (abs(targetX - wokenX) > WAKE || abs(targetY - wokenY) > WAKE) {
            wokenX = targetX
            wokenY = targetY
            onAim?.invoke()
        }
    }

    companion object {
        /** Where the light sits with the phone held still: where the creases catch it best. */
        const val REST_X = 0.2f
        const val REST_Y = -0.4f

        /** How far a full tilt moves the light from rest. */
        const val AMPLITUDE = 0.55f

        /** Target movement, in tilt units, worth waking the render thread for: about a third of a degree. */
        const val WAKE = 0.01f
    }
}

/**
 * EGL on its own thread, paced by the display and asleep whenever the light has arrived: a phone held still
 * draws nothing. At most sixty frames a second, and movement under [IDLE] is not drawn at all, which keeps
 * hand tremor from recompositing the window.
 */
private class WalletLeatherLoop(
    private val surface: Surface,
    assets: AssetManager,
    private val light: WalletLight
) : Choreographer.FrameCallback {
    private val renderer = WalletLeatherRenderer(assets)
    private val thread = HandlerThread("wallet-gl").apply { start() }
    private val handler = Handler(thread.looper)

    private var display: EGLDisplay = EGL14.EGL_NO_DISPLAY
    private var context: EGLContext = EGL14.EGL_NO_CONTEXT
    private var eglSurface: EGLSurface = EGL14.EGL_NO_SURFACE
    private var running = false
    private var scheduled = false

    private var width = 0
    private var height = 0
    private var valueX = WalletLight.REST_X
    private var valueY = WalletLight.REST_Y
    private var lastFrame = 0L
    private var drawnAt = 0L
    private var drawnWidth = -1
    private var drawnHeight = -1
    private var drawnX = Float.NaN
    private var drawnY = Float.NaN
    private var capturedAt = -1L
    private val capture = Runnable { captureIfSettled() }

    fun start(width: Int, height: Int) = handler.post {
        this.width = width
        this.height = height

        val ready = runCatching {
            setUpEgl()
            renderer.init()
        }.onFailure { Timber.e(it, "WalletLeather: could not set up the card renderer") }.isSuccess

        if (!ready) return@post

        valueX = light.targetX
        valueY = light.targetY
        running = true
        light.onAim = { handler.post { schedule() } }
        schedule()
    }

    fun resize(width: Int, height: Int) = handler.post {
        this.width = width
        this.height = height
        schedule()
    }

    /** Blocks until GL is torn down, so the surface is never used after it is destroyed. */
    fun stop() {
        val done = CountDownLatch(1)

        handler.post {
            light.onAim = null
            running = false
            handler.removeCallbacks(capture)
            Choreographer.getInstance().removeFrameCallback(this)

            if (context != EGL14.EGL_NO_CONTEXT) {
                renderer.release()
                EGL14.eglMakeCurrent(display, EGL14.EGL_NO_SURFACE, EGL14.EGL_NO_SURFACE, EGL14.EGL_NO_CONTEXT)
                EGL14.eglDestroySurface(display, eglSurface)
                EGL14.eglDestroyContext(display, context)
                EGL14.eglTerminate(display)
            }

            done.countDown()
        }

        done.await()
        thread.quitSafely()
    }

    private fun schedule() {
        if (!running || scheduled) return

        scheduled = true
        lastFrame = 0L
        Choreographer.getInstance().postFrameCallback(this)
    }

    override fun doFrame(frameTimeNanos: Long) {
        scheduled = false

        if (!running) return

        val seconds = if (lastFrame == 0L) 0f else ((frameTimeNanos - lastFrame) / NANOSECONDS).coerceIn(0f, LONGEST)
        lastFrame = frameTimeNanos

        val ease = 1f - exp(-seconds * SMOOTHING)
        val targetX = light.targetX
        val targetY = light.targetY

        valueX += (targetX - valueX) * ease
        valueY += (targetY - valueY) * ease

        val resized = width != drawnWidth || height != drawnHeight
        val moved = abs(valueX - drawnX) > IDLE || abs(valueY - drawnY) > IDLE
        val due = frameTimeNanos - drawnAt >= MIN_INTERVAL

        if (resized || (moved && due)) {
            renderer.draw(width, height, valueX, valueY)
            EGL14.eglSwapBuffers(display, eglSurface)
            drawnWidth = width
            drawnHeight = height
            drawnX = valueX
            drawnY = valueY
            drawnAt = frameTimeNanos
        }

        val arrived = abs(targetX - drawnX) <= IDLE && abs(targetY - drawnY) <= IDLE

        if (arrived && drawnAt != capturedAt) {
            handler.removeCallbacks(capture)
            handler.postDelayed(capture, SNAPSHOT_DELAY_MILLIS)
        }

        if (!arrived) {
            scheduled = true
            Choreographer.getInstance().postFrameCallback(this)
        }
    }

    /** Only once the light has stayed put: a readback per small settle was most of the idle cost. */
    private fun captureIfSettled() {
        if (!running || scheduled || drawnAt == capturedAt) return

        // Redrawn into the back buffer, because the swap left its contents undefined.
        renderer.draw(width, height, drawnX, drawnY)
        WalletLeatherSnapshot.capture(width, height)
        capturedAt = drawnAt
    }

    private fun setUpEgl() {
        display = EGL14.eglGetDisplay(EGL14.EGL_DEFAULT_DISPLAY)

        val version = IntArray(2)
        check(EGL14.eglInitialize(display, version, 0, version, 1)) { "eglInitialize failed" }

        val attributes = intArrayOf(
            EGL14.EGL_RENDERABLE_TYPE, EGLExt.EGL_OPENGL_ES3_BIT_KHR,
            EGL14.EGL_RED_SIZE, 8,
            EGL14.EGL_GREEN_SIZE, 8,
            EGL14.EGL_BLUE_SIZE, 8,
            EGL14.EGL_ALPHA_SIZE, 8,
            EGL14.EGL_NONE
        )
        val configs = arrayOfNulls<EGLConfig>(1)
        val count = IntArray(1)

        check(EGL14.eglChooseConfig(display, attributes, 0, configs, 0, 1, count, 0) && count[0] > 0) {
            "no RGBA8 ES3 config"
        }

        context = EGL14.eglCreateContext(
            display,
            configs[0],
            EGL14.EGL_NO_CONTEXT,
            intArrayOf(EGL14.EGL_CONTEXT_CLIENT_VERSION, 3, EGL14.EGL_NONE),
            0
        )
        // The default, non-sRGB colour space: the shader's values are already sRGB.
        eglSurface = EGL14.eglCreateWindowSurface(display, configs[0], surface, intArrayOf(EGL14.EGL_NONE), 0)

        check(EGL14.eglMakeCurrent(display, eglSurface, eglSurface, context)) { "eglMakeCurrent failed" }
    }

    private companion object {
        const val NANOSECONDS = 1_000_000_000f
        const val LONGEST = 0.1f

        /** Easing rate of the light, per second: the reference's `tilt.smoothing`. */
        const val SMOOTHING = 5f

        /** Light movement, in tilt units, below which no frame is drawn: the reference's `render.idle`. */
        const val IDLE = 0.002f

        /** A little under a sixtieth, so a 120 Hz display draws every second vsync rather than every third. */
        const val MIN_INTERVAL = (1_000_000_000L / 60 * 9) / 10

        const val SNAPSHOT_DELAY_MILLIS = 500L
    }
}
