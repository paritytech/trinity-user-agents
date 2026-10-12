package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

import android.content.Context
import android.graphics.SurfaceTexture
import android.opengl.EGL14
import android.opengl.EGLConfig
import android.opengl.EGLContext
import android.opengl.EGLDisplay
import android.opengl.EGLSurface
import io.paritytech.polkadotapp.common.utils.logFailure
import timber.log.Timber

/**
 * Owns the GL context and the frame loop for one surface.
 *
 * Drives itself rather than being ticked from outside: `eglSwapBuffers` blocks until the next vsync, so the
 * loop is paced by the display, and it parks the moment every spring and the light have settled. Motion
 * wakes it — a view that only sampled attitude inside its own draw would go to sleep and freeze the light.
 */
class CoinageRenderThread(
    context: Context,
    private val surface: SurfaceTexture,
    private val report: (CoinageCoinsMetrics) -> Unit
) : Thread("coinage-render") {
    private val appContext = context.applicationContext
    private val tilt = CoinageTilt(context)
    private val field = CoinageCoinField()

    private val lock = Object()
    private var running = true
    private var awake = true
    private var isVisible = true

    private var pixelWidth = 0
    private var pixelHeight = 0
    private var density = 1f
    private var coins: List<CoinageScene.Coin> = emptyList()
    private var isExpanded = false
    private var areaWidth = 0f
    private var stripHeight = CoinageStripLayout.Options().height
    private var gridBudget = DEFAULT_GRID_BUDGET
    private var needsRetarget = true

    private var display: EGLDisplay = EGL14.EGL_NO_DISPLAY
    private var eglContext: EGLContext = EGL14.EGL_NO_CONTEXT
    private var eglSurface: EGLSurface = EGL14.EGL_NO_SURFACE
    private var renderer: CoinageGlRenderer? = null

    @Volatile
    private var bundle: CoinageAssetBundle? = null

    private var lastFrame = 0L

    fun resize(width: Int, height: Int, density: Float) = synchronized(lock) {
        if (width == pixelWidth && height == pixelHeight && density == this.density) return@synchronized

        pixelWidth = width
        pixelHeight = height
        this.density = density
        needsRetarget = true
        wake()
    }

    fun update(
        coins: List<CoinageScene.Coin>,
        isExpanded: Boolean,
        areaWidth: Float,
        stripHeight: Float,
        gridBudget: Float
    ) = synchronized(lock) {
        if (coins == this.coins &&
            isExpanded == this.isExpanded &&
            areaWidth == this.areaWidth &&
            stripHeight == this.stripHeight &&
            gridBudget == this.gridBudget
        ) {
            return@synchronized
        }

        this.coins = coins
        this.isExpanded = isExpanded
        this.areaWidth = areaWidth
        this.stripHeight = stripHeight
        this.gridBudget = gridBudget
        needsRetarget = true
        wake()
    }

    /**
     * Follows the card on and off the screen.
     *
     * Nothing here is driven by a system-owned display link, so nothing pauses it for us: backgrounding the
     * app leaves the view attached, which used to leave the sensor registered and the loop free to draw into
     * a surface no one was looking at. iOS gets this for free from `CADisplayLink`; a hand-rolled loop has
     * to be told.
     */
    fun setVisible(visible: Boolean) = synchronized(lock) {
        if (visible == isVisible) return@synchronized

        isVisible = visible

        if (visible) {
            tilt.start()
            lastFrame = 0L
            wake()
        } else {
            tilt.stop()
            lock.notifyAll()
        }
    }

    fun shutdown() {
        synchronized(lock) {
            running = false
            lock.notifyAll()
        }

        runCatching { join(SHUTDOWN_WAIT) }
    }

    override fun run() {
        if (!initialiseEgl()) return

        tilt.onMove = { synchronized(lock) { wake() } }
        tilt.start()

        CoinageAssetLoader.load(appContext) { decoded ->
            bundle = decoded
            synchronized(lock) { wake() }
        }

        try {
            loop()
        } catch (error: Throwable) {
            Timber.e(error, "CoinageRenderThread: the frame loop stopped")
        } finally {
            tilt.stop()
            tilt.onMove = null
            renderer?.release()
            releaseEgl()
        }
    }

    private fun loop() {
        while (true) {
            synchronized(lock) {
                while (running && (!awake || !isVisible)) lock.wait()

                if (!running) return
            }

            if (!frame()) {
                synchronized(lock) { if (!needsRetarget) awake = false }
            }
        }
    }

    /** Returns whether anything is still moving, which is what keeps the loop awake. */
    private fun frame(): Boolean {
        val ready = renderer ?: bundle?.let { decoded ->
            runCatching { CoinageGlRenderer.create(decoded) }
                .logFailure("CoinageRenderThread: could not build the coin renderer")
                .getOrNull()
                ?.also { renderer = it }
        } ?: return false

        val width: Int
        val height: Int
        val scale: Float
        val retarget: Boolean

        synchronized(lock) {
            width = pixelWidth
            height = pixelHeight
            scale = density
            retarget = needsRetarget
            needsRetarget = false
        }

        if (width <= 0 || height <= 0) return false

        if (retarget) retarget(ready.designs)

        val moved = advance()
        val batches = CoinageScene.batches(field, ready.designs)

        ready.draw(
            batches = batches,
            viewportWidth = width / scale,
            viewportHeight = height / scale,
            pixelWidth = width,
            pixelHeight = height,
            density = scale,
            light = tilt.turn
        )

        EGL14.eglSwapBuffers(display, eglSurface)

        return field.isMoving || moved
    }

    /**
     * A real elapsed time rather than a nominal frame: the spring is exact for any step, and a dropped frame
     * should not slow the motion down.
     */
    private fun advance(): Boolean {
        val now = System.nanoTime()
        val elapsed = if (lastFrame == 0L) {
            NOMINAL_FRAME
        } else {
            ((now - lastFrame) / NANOSECONDS).coerceAtMost(LONGEST_FRAME)
        }
        lastFrame = now

        field.advance(elapsed)

        return tilt.advance(elapsed)
    }

    /**
     * Lays out against the width the card measured, not against the surface's own.
     *
     * The card works the grid's height out for itself so it can size the surface before anything asks for the
     * grid, and the two have to be the same grid: the same width in, the same columns and the same height
     * out. Deriving one from the surface pixels and the other from the composable's constraints agreed to
     * within a rounding, which is exactly the kind of near-agreement that flips a column count at one width
     * in a hundred.
     */
    private fun retarget(designs: List<CoinageAssetStore.Design>) {
        val snapshot: List<CoinageScene.Coin>
        val expanded: Boolean
        val width: Float
        val strip: Float
        val budget: Float

        synchronized(lock) {
            snapshot = coins
            expanded = isExpanded
            width = areaWidth
            strip = stripHeight
            budget = gridBudget
        }

        if (width <= 0f) return

        val result = CoinageArrangement.targets(
            coins = snapshot,
            arrangement = if (expanded) CoinageArrangement.GRID else CoinageArrangement.STRIP,
            areaWidth = width,
            areaHeight = budget,
            designs = designs,
            stripHeight = strip
        )

        field.retarget(
            targets = result.targets,
            spawnEdge = width,
            // Spreading out, the last coins have furthest to travel; gathering back in, the first ones do.
            // Either way the long haul sets off first.
            stagger = if (expanded) CoinageCoinField.Stagger.FROM_BACK else CoinageCoinField.Stagger.FROM_FRONT
        )

        report(CoinageCoinsMetrics(blocks = result.blocks, runs = result.runs))
        lastFrame = 0L
    }

    private fun wake() {
        awake = true
        lock.notifyAll()
    }

    private fun initialiseEgl(): Boolean {
        display = EGL14.eglGetDisplay(EGL14.EGL_DEFAULT_DISPLAY)

        if (display == EGL14.EGL_NO_DISPLAY) return false

        val version = IntArray(2)

        if (!EGL14.eglInitialize(display, version, 0, version, 1)) return false

        val config = chooseConfig() ?: return false

        eglContext = EGL14.eglCreateContext(
            display,
            config,
            EGL14.EGL_NO_CONTEXT,
            intArrayOf(EGL14.EGL_CONTEXT_CLIENT_VERSION, 3, EGL14.EGL_NONE),
            0
        )

        if (eglContext == EGL14.EGL_NO_CONTEXT) return false

        eglSurface = EGL14.eglCreateWindowSurface(
            display,
            config,
            surface,
            intArrayOf(EGL14.EGL_NONE),
            0
        )

        if (eglSurface == EGL14.EGL_NO_SURFACE) return false

        return EGL14.eglMakeCurrent(display, eglSurface, eglSurface, eglContext)
    }

    /** Multisampled if the device offers it, which every phone this app runs on does. */
    private fun chooseConfig(): EGLConfig? = pickConfig(SAMPLES) ?: pickConfig(0)

    private fun pickConfig(samples: Int): EGLConfig? {
        val attributes = intArrayOf(
            EGL14.EGL_RENDERABLE_TYPE, EGL_OPENGL_ES3_BIT,
            EGL14.EGL_SURFACE_TYPE, EGL14.EGL_WINDOW_BIT,
            EGL14.EGL_RED_SIZE, 8,
            EGL14.EGL_GREEN_SIZE, 8,
            EGL14.EGL_BLUE_SIZE, 8,
            EGL14.EGL_ALPHA_SIZE, 8,
            EGL14.EGL_DEPTH_SIZE, 16,
            EGL14.EGL_SAMPLE_BUFFERS, if (samples > 0) 1 else 0,
            EGL14.EGL_SAMPLES, samples,
            EGL14.EGL_NONE
        )

        val configs = arrayOfNulls<EGLConfig>(1)
        val found = IntArray(1)

        if (!EGL14.eglChooseConfig(display, attributes, 0, configs, 0, 1, found, 0)) return null

        return configs[0].takeIf { found[0] > 0 }
    }

    private fun releaseEgl() {
        if (display == EGL14.EGL_NO_DISPLAY) return

        EGL14.eglMakeCurrent(display, EGL14.EGL_NO_SURFACE, EGL14.EGL_NO_SURFACE, EGL14.EGL_NO_CONTEXT)

        if (eglSurface != EGL14.EGL_NO_SURFACE) EGL14.eglDestroySurface(display, eglSurface)
        if (eglContext != EGL14.EGL_NO_CONTEXT) EGL14.eglDestroyContext(display, eglContext)

        EGL14.eglTerminate(display)
        surface.release()
    }

    private companion object {
        const val EGL_OPENGL_ES3_BIT = 0x0040
        const val SAMPLES = 4
        const val NOMINAL_FRAME = 1f / 60
        const val LONGEST_FRAME = 1f / 20
        const val NANOSECONDS = 1_000_000_000f
        const val SHUTDOWN_WAIT = 500L
    }
}

/**
 * Where the coins came out, so the card can rule their runs and label their blocks.
 *
 * Not the height: the card works that out for itself before the coins are asked to move, which is what keeps
 * the drawing surface from being resized under the flight.
 */
data class CoinageCoinsMetrics(
    val blocks: List<CoinageGridLayout.Block> = emptyList(),
    val runs: List<CoinageStripLayout.Span> = emptyList()
)
