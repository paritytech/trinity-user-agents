package io.paritytech.polkadotapp.feature_products_impl.presentation.bot.footer.web

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Color
import android.graphics.Rect
import android.os.Build
import android.os.Message
import android.os.SystemClock
import android.view.Gravity
import android.view.HapticFeedbackConstants
import android.view.MotionEvent
import android.webkit.RenderProcessGoneDetail
import android.webkit.WebChromeClient
import android.webkit.WebView
import android.widget.FrameLayout
import androidx.core.view.ViewCompat
import io.paritytech.polkadotapp.feature_dotns_api.presentation.DotNsWebViewClient
import io.paritytech.polkadotapp.feature_products_impl.presentation.bot.overlay.ProductOverlayPage
import io.paritytech.polkadotapp.feature_products_impl.presentation.bot.overlay.ProductOverlayTouchRule
import io.paritytech.polkadotapp.feature_products_impl.presentation.bot.overlay.ProductOverlayWebLayout
import timber.log.Timber

/**
 * A transparent webview where the composer would be, showing the product's own page.
 *
 * The page reports through `window.ProductFooter`: an action goes to the product as a press in the room footer, a haptic
 * plays, `sendOverlay` shows another of the product's pages over the whole screen for a moment, and `ready` sizes and
 * reveals it. The webview stays within the footer; the overlay is a webview of its own, and it is only honoured just
 * after a touch on the footer ([ProductOverlayTouchRule]).
 */
@SuppressLint("ViewConstructor")
class ProductRoomFooterWebLayout(
    context: Context,
    private val productId: String,
    private val pageUrl: String,
    private val content: DotNsWebViewClient,
    private val onAction: (ProductFooterMessage.Action) -> Unit,
    private val onHeight: (Int) -> Unit,
    private val onRevealed: () -> Unit = {},
) : FrameLayout(context) {
    private val overlayTouchRule = ProductOverlayTouchRule()
    private var webView: WebView = createWebView()
    private var isRevealed = false
    private var overlay: ProductOverlayWebLayout? = null
    private val revealFallback = Runnable { reveal() }

    init {
        setBackgroundColor(Color.TRANSPARENT)
        addView(webView, webViewLayoutParams())
        webView.loadUrl(pageUrl)
        postDelayed(revealFallback, REVEAL_FALLBACK_MILLIS)
    }

    /** Tells the page something, through `window.productFooter.receive` when it defines one. */
    fun send(event: ProductFooterHostEvent) {
        val url = webView.url ?: return
        if (!ProductRoomFooterWebPage.isProductOrigin(url, productId)) return
        webView.evaluateJavascript(event.script, null)
    }

    fun release() {
        removeCallbacks(revealFallback)
        destroyProductPage(webView)
    }

    override fun onLayout(changed: Boolean, left: Int, top: Int, right: Int, bottom: Int) {
        super.onLayout(changed, left, top, right, bottom)
        // A drag that starts at the screen edge on the slider is the page's, not the system back gesture.
        ViewCompat.setSystemGestureExclusionRects(this, listOf(Rect(0, 0, width, height)))
    }

    private fun handle(message: ProductFooterMessage) {
        when (message) {
            is ProductFooterMessage.Action -> onAction(message)
            is ProductFooterMessage.Haptic -> playHaptic(message.style)
            is ProductFooterMessage.SendOverlay -> showOverlay(message.path)
            is ProductFooterMessage.Ready -> {
                ProductRoomFooterWebPage.height(message.height)?.let(onHeight)
                reveal()
            }
        }
    }

    /**
     * Shows the product's page at [path] over the screen, one at a time. It outlives this footer: a footer the product
     * replaces mid-effect does not cut it short, and it always ends with its page or at its limit.
     */
    private fun showOverlay(path: String) {
        if (!overlayTouchRule.allowsOverlay(SystemClock.uptimeMillis())) {
            Timber.d("Web footer of %s dropped an overlay asked for without a touch on the footer", productId)
            return
        }
        if (overlay?.isShowing == true) {
            Timber.d("Web footer of %s refused an overlay: one is already showing", productId)
            return
        }
        val url = ProductOverlayPage.url(path, productId) ?: run {
            Timber.w("Web footer of %s refused an overlay that is no page of the product: %s", productId, path)
            return
        }
        overlay = ProductOverlayWebLayout.show(this, productId, url, content)
    }

    private fun reveal() {
        removeCallbacks(revealFallback)
        if (isRevealed) return
        isRevealed = true
        webView.animate().alpha(1f).setDuration(REVEAL_ANIMATION_MILLIS).start()
        onRevealed()
    }

    private fun playHaptic(style: String) {
        val constant = when (style) {
            "warning", "error" -> if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) HapticFeedbackConstants.REJECT else HapticFeedbackConstants.LONG_PRESS
            "selection", "light", "soft" -> HapticFeedbackConstants.CLOCK_TICK
            "medium", "heavy", "rigid" -> HapticFeedbackConstants.VIRTUAL_KEY
            else -> if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) HapticFeedbackConstants.CONFIRM else HapticFeedbackConstants.VIRTUAL_KEY
        }
        performHapticFeedback(constant)
    }

    @SuppressLint("ClickableViewAccessibility")
    private fun createWebView(): WebView = WebView(context).apply {
        applyProductPageSettings()
        alpha = 0f
        // Parents that would take the drag for a scroll or a swipe never get to intercept one that starts here.
        setOnTouchListener { view, event ->
            when (event.actionMasked) {
                MotionEvent.ACTION_DOWN -> view.parent?.requestDisallowInterceptTouchEvent(true)
                // A touch that ends on the footer lets its page ask for an overlay for a moment (ProductOverlayTouchRule).
                MotionEvent.ACTION_UP -> {
                    val inside = event.x in 0f..view.width.toFloat() && event.y in 0f..view.height.toFloat()
                    overlayTouchRule.touchEnded(event.eventTime, inside)
                }
            }
            false
        }
        webViewClient = FooterWebViewClient()
        webChromeClient = FooterChromeClient()
        ProductPageBridge.install(this, ProductFooterMessage.BRIDGE_NAME, productId, LABEL) { data ->
            ProductFooterMessage.parse(data)?.let(::handle) ?: Timber.d("Web footer of %s dropped an unreadable message", productId)
        }
    }

    private inner class FooterWebViewClient : ProductPageWebViewClient(productId, content, LABEL) {
        override fun onPageFinished(view: WebView, url: String?) {
            if (view === webView) reveal()
        }

        override fun onRenderProcessGone(view: WebView, detail: RenderProcessGoneDetail?): Boolean {
            if (view === webView) replaceDeadWebView()
            return true
        }
    }

    private class FooterChromeClient : WebChromeClient() {
        override fun onCreateWindow(view: WebView?, isDialog: Boolean, isUserGesture: Boolean, resultMsg: Message?): Boolean = false
    }

    private fun replaceDeadWebView() {
        val dead = webView
        removeView(dead)
        destroyProductPage(dead)
        webView = createWebView()
        addView(webView, webViewLayoutParams())
        isRevealed = false
        webView.loadUrl(pageUrl)
        postDelayed(revealFallback, REVEAL_FALLBACK_MILLIS)
    }

    private fun webViewLayoutParams() = LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT, Gravity.BOTTOM)

    private companion object {
        const val LABEL = "Web footer"
        const val REVEAL_FALLBACK_MILLIS = 2_000L
        const val REVEAL_ANIMATION_MILLIS = 150L
    }
}
