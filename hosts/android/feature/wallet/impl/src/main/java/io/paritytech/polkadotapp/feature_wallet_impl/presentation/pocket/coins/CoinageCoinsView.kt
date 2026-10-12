package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

import android.content.Context
import android.graphics.SurfaceTexture
import android.os.Handler
import android.os.Looper
import android.view.TextureView
import android.widget.FrameLayout

/**
 * Every holding, as real coins, in one of two arrangements.
 *
 * Collapsed, they are the summary strip: a fixed height however many there are, face on with margins while
 * they fit, turning about their vertical axis as they multiply, thinning once fully edge-on. Expanded, the
 * same coins spread into a honeycomb, one cell each however many there are, and the card grows and scrolls.
 *
 * One view, one set of coins. A toggle only moves targets, so the coins fly between the two arrangements
 * rather than one view cutting to another.
 *
 * A [TextureView] rather than a `SurfaceView`: the strip sits inside a rounded, scrolling card, and a surface
 * punches its own hole through the window rather than being composited with what is drawn over and under it.
 *
 * The frame around it is what makes the surface hold still. The card animates *this* view, which is an
 * ordinary one and costs nothing to resize; the texture inside keeps the height the coins were laid out for
 * and is clipped by it. Asking Compose to clip an oversized interop child did not do it — the texture was
 * placed off its own top and the coins vanished under the clip — and a view group clipping its own child is
 * not a thing that can be got wrong.
 */
class CoinageCoinsView(context: Context) : FrameLayout(context), TextureView.SurfaceTextureListener {
    private val main = Handler(Looper.getMainLooper())
    private val texture = TextureView(context)

    private var thread: CoinageRenderThread? = null
    private var coins: List<CoinageScene.Coin> = emptyList()
    private var isExpanded = false
    private var areaWidth = 0f
    private var stripHeight = CoinageStripLayout.Options().height
    private var gridBudget = DEFAULT_GRID_BUDGET

    var onMetrics: ((CoinageCoinsMetrics) -> Unit)? = null

    init {
        clipChildren = true
        texture.isOpaque = false
        texture.surfaceTextureListener = this
        addView(texture, LayoutParams(LayoutParams.MATCH_PARENT, 0))
    }

    fun update(
        coins: List<CoinageScene.Coin>,
        isExpanded: Boolean,
        areaWidth: Float,
        stripHeight: Float,
        gridBudget: Float,
        surfaceHeight: Int
    ) {
        this.coins = coins
        this.isExpanded = isExpanded
        this.areaWidth = areaWidth
        this.stripHeight = stripHeight
        this.gridBudget = gridBudget

        if (texture.layoutParams.height != surfaceHeight) {
            texture.layoutParams = texture.layoutParams.apply { height = surfaceHeight }
        }

        thread?.update(coins, isExpanded, areaWidth, stripHeight, gridBudget)
    }

    override fun onSurfaceTextureAvailable(surface: SurfaceTexture, width: Int, height: Int) {
        val started = CoinageRenderThread(context, surface) { metrics ->
            main.post { onMetrics?.invoke(metrics) }
        }

        thread = started
        started.start()
        started.update(coins, isExpanded, areaWidth, stripHeight, gridBudget)
        started.resize(width, height, resources.displayMetrics.density)
    }

    override fun onSurfaceTextureSizeChanged(surface: SurfaceTexture, width: Int, height: Int) {
        thread?.resize(width, height, resources.displayMetrics.density)
    }

    /** The thread releases the surface itself once its context is gone, so this never does. */
    override fun onSurfaceTextureDestroyed(surface: SurfaceTexture): Boolean {
        thread?.shutdown()
        thread = null

        return false
    }

    override fun onSurfaceTextureUpdated(surface: SurfaceTexture) = Unit

    /**
     * Covers the window going away as well as this view being hidden, which is what makes it the right hook
     * for the app being backgrounded — the view stays attached through that, so the surface callbacks say
     * nothing about it.
     */
    override fun onVisibilityAggregated(isVisible: Boolean) {
        super.onVisibilityAggregated(isVisible)

        thread?.setVisible(isVisible)
    }
}
