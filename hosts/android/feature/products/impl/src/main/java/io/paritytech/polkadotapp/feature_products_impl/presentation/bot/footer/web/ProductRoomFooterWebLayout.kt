package io.paritytech.polkadotapp.feature_products_impl.presentation.bot.footer.web

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Color
import android.graphics.Rect
import android.net.Uri
import android.os.Build
import android.os.Message
import android.view.Gravity
import android.view.HapticFeedbackConstants
import android.view.MotionEvent
import android.view.View
import android.view.ViewGroup
import android.webkit.JavascriptInterface
import android.webkit.RenderProcessGoneDetail
import android.webkit.WebChromeClient
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.FrameLayout
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import io.paritytech.polkadotapp.feature_dotns_api.presentation.DotNsWebViewClient
import timber.log.Timber
import java.io.ByteArrayInputStream

/**
 * A transparent webview where the composer would be, showing the product's own page.
 *
 * The page reports through `window.ProductFooter`: an action goes to the product as a press in the room footer, a haptic
 * plays, `overlay` lets the page draw over the chat above it for a moment, and `ready` sizes and reveals it.
 */
@SuppressLint("ViewConstructor")
class ProductRoomFooterWebLayout(
    context: Context,
    private val productId: String,
    private val pageUrl: String,
    private val content: DotNsWebViewClient,
    private val onAction: (ProductFooterMessage.Action) -> Unit,
    private val onHeight: (Int) -> Unit,
) : FrameLayout(context) {
    private val origin = ProductRoomFooterWebPage.origin(productId)
    private var webView: WebView = createWebView()
    private var isOverlaying = false
    private val hideOverlay = Runnable { setOverlay(false) }
    private val revealFallback = Runnable { reveal() }

    init {
        setBackgroundColor(Color.TRANSPARENT)
        clipChildren = false
        clipToPadding = false
        addView(webView, webViewLayoutParams(LayoutParams.MATCH_PARENT))
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
        removeCallbacks(hideOverlay)
        removeCallbacks(revealFallback)
        destroy(webView)
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        // The interop holder clips its child by default, which would cut the overlay off at the footer's top.
        (parent as? ViewGroup)?.clipChildren = false
    }

    override fun onDetachedFromWindow() {
        setOverlay(false)
        super.onDetachedFromWindow()
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
            is ProductFooterMessage.Overlay -> setOverlay(message.isOn)
            is ProductFooterMessage.Ready -> {
                ProductRoomFooterWebPage.height(message.height)?.let(onHeight)
                reveal()
            }
        }
    }

    /**
     * Lets the transparent webview draw over the chat above the footer, up to the status bar.
     *
     * Only the drawing grows: this view keeps the footer's bounds, so a touch above the footer still lands on the chat
     * and the page cannot take touches there.
     */
    private fun setOverlay(isOn: Boolean) {
        removeCallbacks(hideOverlay)
        val extra = if (isOn && isAttachedToWindow) overlayExtraHeight() else 0
        isOverlaying = extra > 0
        val params = webView.layoutParams as LayoutParams
        val wanted = if (isOverlaying) height + extra else LayoutParams.MATCH_PARENT
        if (params.height != wanted) {
            params.height = wanted
            webView.layoutParams = params
        }
        if (isOverlaying) postDelayed(hideOverlay, OVERLAY_LIMIT_MILLIS)
    }

    private fun overlayExtraHeight(): Int {
        val location = IntArray(2)
        getLocationInWindow(location)
        val statusBar = ViewCompat.getRootWindowInsets(this)?.getInsets(WindowInsetsCompat.Type.statusBars())?.top ?: 0
        return (location[1] - statusBar).coerceAtLeast(0)
    }

    private fun reveal() {
        removeCallbacks(revealFallback)
        if (webView.alpha >= 1f) return
        webView.animate().alpha(1f).setDuration(REVEAL_ANIMATION_MILLIS).start()
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

    @SuppressLint("SetJavaScriptEnabled", "ClickableViewAccessibility")
    private fun createWebView(): WebView = WebView(context).apply {
        setBackgroundColor(Color.TRANSPARENT)
        alpha = 0f
        isVerticalScrollBarEnabled = false
        isHorizontalScrollBarEnabled = false
        overScrollMode = View.OVER_SCROLL_NEVER
        isLongClickable = false
        setOnLongClickListener { true }
        isHapticFeedbackEnabled = false
        settings.apply {
            javaScriptEnabled = true
            domStorageEnabled = false
            allowFileAccess = false
            allowContentAccess = false
            javaScriptCanOpenWindowsAutomatically = false
            setSupportMultipleWindows(false)
            setGeolocationEnabled(false)
            setSupportZoom(false)
            builtInZoomControls = false
            displayZoomControls = false
            mediaPlaybackRequiresUserGesture = true
        }
        // Parents that would take the drag for a scroll or a swipe never get to intercept one that starts here.
        setOnTouchListener { view, event ->
            if (event.actionMasked == MotionEvent.ACTION_DOWN) view.parent?.requestDisallowInterceptTouchEvent(true)
            false
        }
        webViewClient = FooterWebViewClient()
        webChromeClient = FooterChromeClient()
        installBridge(this)
    }

    private fun installBridge(view: WebView) {
        if (WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)) {
            // Injected only into documents of the product's origin; the listener also sees which frame posted.
            WebViewCompat.addWebMessageListener(view, ProductFooterMessage.BRIDGE_NAME, setOf(origin)) { _, message, sourceOrigin, isMainFrame, _ ->
                receive(message.data, sourceOrigin, isMainFrame)
            }
        } else {
            // No frame can host a sender here: navigation stays on the product's origin and the document's CSP refuses
            // iframes, so the posting document is the main frame, which is checked against the origin on the UI thread.
            view.addJavascriptInterface(LegacyBridge(), ProductFooterMessage.BRIDGE_NAME)
        }
    }

    private fun receive(data: String?, sourceOrigin: Uri?, isMainFrame: Boolean) {
        val fromProductPage = isMainFrame && sourceOrigin?.toString()?.trimEnd('/').equals(origin, ignoreCase = true)
        if (!fromProductPage) {
            Timber.w("Web footer of %s dropped a message from outside the product page", productId)
            return
        }
        val parsed = data?.let(ProductFooterMessage::parse) ?: run {
            Timber.d("Web footer of %s dropped an unreadable message", productId)
            return
        }
        handle(parsed)
    }

    private inner class LegacyBridge {
        @JavascriptInterface
        fun postMessage(data: String?) {
            post {
                val url = webView.url
                val sourceOrigin = url?.takeIf { ProductRoomFooterWebPage.isProductOrigin(it, productId) }?.let { Uri.parse(origin) }
                receive(data, sourceOrigin, isMainFrame = true)
            }
        }
    }

    private inner class FooterWebViewClient : WebViewClient() {
        override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
            val url = request.url.toString()
            val allowed = ProductRoomFooterWebPage.isAllowedNavigation(url, productId)
            if (!allowed) Timber.w("Web footer of %s refused to leave the product origin for %s", productId, url)
            return !allowed
        }

        override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? {
            val url = request.url
            if (!ProductRoomFooterWebPage.isProductOrigin(url.toString(), productId)) {
                Timber.w("Web footer of %s refused a request off the product origin: %s", productId, url)
                return refusedResponse()
            }
            // A missing footer page must stay missing, not fall back to the product's SPA index.
            if (request.isForMainFrame) {
                val archive = content.resolveHostFile(url)
                if (content.resolveFile(archive, url.path.orEmpty().removePrefix("/")) == null) return refusedResponse()
            }
            // Nothing on the product's origin is ever fetched from the network.
            return content.shouldInterceptRequest(view, request) ?: refusedResponse()
        }

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
        destroy(dead)
        webView = createWebView()
        addView(webView, webViewLayoutParams(LayoutParams.MATCH_PARENT))
        isOverlaying = false
        webView.loadUrl(pageUrl)
        postDelayed(revealFallback, REVEAL_FALLBACK_MILLIS)
    }

    private fun destroy(view: WebView) {
        view.stopLoading()
        view.webViewClient = WebViewClient()
        (view.parent as? ViewGroup)?.removeView(view)
        view.destroy()
    }

    private fun webViewLayoutParams(height: Int) = LayoutParams(LayoutParams.MATCH_PARENT, height, Gravity.BOTTOM)

    private fun refusedResponse() = WebResourceResponse("text/plain", "UTF-8", ByteArrayInputStream(ByteArray(0))).apply {
        setStatusCodeAndReasonPhrase(404, "Not Found")
    }

    private companion object {
        const val OVERLAY_LIMIT_MILLIS = 6_000L
        const val REVEAL_FALLBACK_MILLIS = 2_000L
        const val REVEAL_ANIMATION_MILLIS = 150L
    }
}
