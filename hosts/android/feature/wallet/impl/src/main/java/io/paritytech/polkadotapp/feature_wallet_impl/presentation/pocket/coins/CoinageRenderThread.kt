package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

import android.content.Context
import android.graphics.SurfaceTexture
import android.opengl.EGL14
import android.opengl.EGLConfig
import android.opengl.EGLContext
import android.opengl.EGLDisplay
import android.opengl.EGLSurface
import android.opengl.GLES30
import android.os.Handler
import android.os.HandlerThread
import android.view.Choreographer
import timber.log.Timber

/**
 * Owns the GL context and the frame loop for one surface.
 *
 * Each frame waits for a vsync from a Choreographer on a thread of its own. Swapping into a `TextureView`'s
 * surface does not block — the texture drops frames it has no time to show — so a loop paced by the swap
 * alone spun at over a thousand frames a second. Not the main thread's Choreographer: a busy UI frame would
 * hold the coins back with it. The loop parks the moment every spring and the light have settled; motion
 * wakes it, since a view that only sampled attitude inside its own draw would sleep and freeze the light.
 *
 * At most two frames are in flight: before drawing, it waits for the frame before last to finish on the GPU.
 * A third only queues behind a slow GPU and shows the light a frame later. The wait is on this thread alone;
 * nothing on the main thread ever waits for the GPU.
 */
class CoinageRenderThread(
    context: Context,
    private val surface: SurfaceTexture,
    private val report: (CoinageCoinsMetrics) -> Unit
) : Thread("coinage-render") {
    private val appContext = context.applicationContext
    private val tilt = CoinageTilt(context)
    private val fences = LongArray(FRAMES_IN_FLIGHT)
    private var fenceIndex = 0

    private val vsyncThread = HandlerThread("coinage-vsync").also { it.start() }
    private val vsyncHandler = Handler(vsyncThread.looper)
    private var vsyncs = 0L
    private var vsyncRequested = false
    private val requestVsync = Runnable { Choreographer.getInstance().postFrameCallback(onVsync) }
    private val onVsync = Choreographer.FrameCallback {
        synchronized(lock) {
            vsyncs++
            vsyncRequested = false
            lock.notifyAll()
        }
    }
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
    private var focus: CoinageArrangement.Focus? = null
    private var window = 0f
    private var coinSet = CoinageScene.CoinSet.SHIPPED
    private var needsRetarget = true
    private var needsFollow = false
    private var needsDraw = false
    private val taps = ArrayList<Tap>()

    /** How tall the coins alone came out at the last retarget, for placing the band under or over them. */
    private var coinsHeight = 0f
    private var focusTop: Float? = null

    /**
     * The window the last swapped frame was drawn for, in points. The view moves the surface to it as that
     * frame is latched, so the coins and the surface's position change together rather than a frame apart.
     */
    @Volatile
    var drawnWindow = 0f
        private set

    private var display: EGLDisplay = EGL14.EGL_NO_DISPLAY
    private var eglContext: EGLContext = EGL14.EGL_NO_CONTEXT
    private var eglSurface: EGLSurface = EGL14.EGL_NO_SURFACE
    private var renderer: CoinageGlRenderer? = null

    @Volatile
    private var bundle: CoinageAssetBundle? = null

    private var lastFrame = 0L
    private var lastShownFrame: ShownFrame? = null

    @Volatile
    var timings: CoinageFrameTimings? = null
        set(value) {
            field = value
            tilt.onReading = value?.let { it::onReading }
        }

    /**
     * Only a width change re-aims the coins: both arrangements are laid out against the width alone, and
     * re-aiming on a height change restarted every coin's stagger, which read as a hitch.
     */
    fun resize(width: Int, height: Int, density: Float) = synchronized(lock) {
        if (width == pixelWidth && height == pixelHeight && density == this.density) return@synchronized

        if (width != pixelWidth || density != this.density) needsRetarget = true

        pixelWidth = width
        pixelHeight = height
        this.density = density
        wake()
    }

    fun update(
        coins: List<CoinageScene.Coin>,
        isExpanded: Boolean,
        areaWidth: Float,
        stripHeight: Float,
        gridBudget: Float,
        focus: CoinageArrangement.Focus?,
        window: Float,
        coinSet: CoinageScene.CoinSet
    ) = synchronized(lock) {
        val relaid = coins != this.coins ||
            isExpanded != this.isExpanded ||
            areaWidth != this.areaWidth ||
            stripHeight != this.stripHeight ||
            gridBudget != this.gridBudget ||
            focus?.id != this.focus?.id ||
            focus?.band != this.focus?.band ||
            coinSet != this.coinSet
        val scrolled = !relaid && focus != this.focus
        val redrawn = window != this.window

        this.coins = coins
        this.isExpanded = isExpanded
        this.areaWidth = areaWidth
        this.stripHeight = stripHeight
        this.gridBudget = gridBudget
        this.focus = focus
        this.window = window
        this.coinSet = coinSet

        if (relaid) needsRetarget = true
        if (scrolled) needsFollow = true

        if (redrawn) needsDraw = true
        if (relaid || scrolled || redrawn) wake()
    }

    /** Answers on the render thread, against where every coin has got to by the next frame. */
    fun tap(x: Float, y: Float, onResult: (String?) -> Unit) = synchronized(lock) {
        taps += Tap(x, y, onResult)
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
        vsyncThread.quitSafely()
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
            fences.forEach { if (it != 0L) GLES30.glDeleteSync(it) }
            renderer?.release()
            releaseEgl()
        }
    }

    private fun loop() {
        while (true) {
            synchronized(lock) {
                while (running && (!awake || !isVisible)) lock.wait()

                if (!running) return

                val seen = vsyncs

                if (!vsyncRequested) {
                    vsyncRequested = true
                    vsyncHandler.post(requestVsync)
                }

                while (running && vsyncs == seen) lock.wait()

                if (!running) return
            }

            if (!frame()) {
                synchronized(lock) {
                    if (!needsRetarget && !needsFollow && !needsDraw && taps.isEmpty()) awake = false
                }
            }
        }
    }

    /** Returns whether anything is still moving, which is what keeps the loop awake. */
    private fun frame(): Boolean {
        val ready = renderer ?: bundle?.let { decoded ->
            CoinageGlRenderer.create(decoded, ::isShutDown)?.also { renderer = it }
        } ?: return false

        val width: Int
        val height: Int
        val scale: Float
        val retarget: Boolean
        val follow: Boolean
        val offset: Float
        val set: CoinageScene.CoinSet
        val pending: List<Tap>

        synchronized(lock) {
            width = pixelWidth
            height = pixelHeight
            scale = density
            retarget = needsRetarget
            needsRetarget = false
            follow = needsFollow
            needsFollow = false
            needsDraw = false
            offset = window
            set = coinSet
            pending = taps.toList()
            taps.clear()
        }

        if (width <= 0 || height <= 0) return false

        awaitFrameBeforeLast()

        val drawn = System.nanoTime()
        val light = tilt.turn

        if (retarget) retarget(ready.designs) else if (follow) follow()

        for (tap in pending) tap.onResult(CoinageScene.coinAt(tap.x, tap.y, field, set))

        val coinsMove = field.isMoving
        val moved = advance(drawn)
        val frame = ShownFrame(tilt.turn, offset, width, height, scale, set)

        if (!coinsMove && frame == lastShownFrame) return field.isMoving || moved

        val batches = CoinageScene.batches(field, ready.designs, offset, set)

        ready.draw(
            batches = batches,
            viewportWidth = width / scale,
            viewportHeight = height / scale,
            pixelWidth = width,
            pixelHeight = height,
            density = scale,
            light = frame.light,
            set = set
        )

        fences[fenceIndex] = GLES30.glFenceSync(GLES30.GL_SYNC_GPU_COMMANDS_COMPLETE, 0)
        fenceIndex = (fenceIndex + 1) % FRAMES_IN_FLIGHT

        EGL14.eglSwapBuffers(display, eglSurface)
        drawnWindow = offset
        lastShownFrame = frame

        timings?.onSwapped(drawn, System.nanoTime(), tookReading = tilt.turn != light)

        return field.isMoving || moved
    }

    /** Sampled after the wait, so the frame takes the freshest reading there is. */
    private fun awaitFrameBeforeLast() {
        val fence = fences[fenceIndex]

        if (fence == 0L) return

        GLES30.glClientWaitSync(fence, GLES30.GL_SYNC_FLUSH_COMMANDS_BIT, FENCE_TIMEOUT)
        GLES30.glDeleteSync(fence)
        fences[fenceIndex] = 0L
    }

    /**
     * A real elapsed time rather than a nominal frame: the spring is exact for any step, and a dropped frame
     * should not slow the motion down.
     */
    private fun advance(now: Long): Boolean {
        val elapsed = if (lastFrame == 0L) {
            NOMINAL_FRAME
        } else {
            ((now - lastFrame) / NANOSECONDS).coerceAtMost(LONGEST_FRAME)
        }
        lastFrame = now

        field.advance(elapsed)

        return tilt.advance(now)
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
        val picked: CoinageArrangement.Focus?
        val set: CoinageScene.CoinSet

        synchronized(lock) {
            snapshot = coins
            expanded = isExpanded
            width = areaWidth
            strip = stripHeight
            budget = gridBudget
            picked = focus
            set = coinSet
        }

        if (width <= 0f) return

        val result = CoinageArrangement.targets(
            coins = snapshot,
            arrangement = if (expanded) CoinageArrangement.GRID else CoinageArrangement.STRIP,
            areaWidth = width,
            areaHeight = budget,
            designs = designs,
            stripHeight = strip,
            focus = picked,
            set = set
        )

        field.retarget(
            targets = result.targets,
            spawnEdge = width,
            // Spreading out, the last coins have furthest to travel; gathering back in, the first ones do.
            // Either way the long haul sets off first.
            stagger = if (expanded) CoinageCoinField.Stagger.FROM_BACK else CoinageCoinField.Stagger.FROM_FRONT
        )

        coinsHeight = result.coinsHeight
        focusTop = result.focusTop
        report(CoinageCoinsMetrics(blocks = result.blocks, runs = result.runs))
        lastFrame = 0L
    }

    /**
     * Keeps the picked coin at the foot of the screen as the page scrolls: the coin alone is re-aimed, on its
     * spring and with no stagger, and the other coins stay where they are.
     */
    private fun follow() {
        val picked = synchronized(lock) { focus } ?: return

        if (field.members.none { it.coin.id == picked.id }) return

        val top = CoinageArrangement.bandTop(picked, coinsHeight)

        if (top == focusTop) return

        focusTop = top
        field.move(picked.id, CoinageArrangement.pickedCentreX(), CoinageArrangement.pickedCentreY(top))
    }

    private fun wake() {
        awake = true
        lock.notifyAll()
    }

    private fun isShutDown(): Boolean = synchronized(lock) { !running }

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
        const val FRAMES_IN_FLIGHT = 2
        const val FENCE_TIMEOUT = 100_000_000L
    }
}

private class Tap(val x: Float, val y: Float, val onResult: (String?) -> Unit)

private data class ShownFrame(
    val light: CoinageTilt.Turn,
    val offset: Float,
    val width: Int,
    val height: Int,
    val density: Float,
    val set: CoinageScene.CoinSet
)

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
