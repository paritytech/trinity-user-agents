package io.paritytech.polkadotapp.feature_products_impl.presentation.bot.overlay

import android.annotation.SuppressLint
import android.app.Activity
import android.content.Context
import android.content.ContextWrapper
import android.graphics.Color
import android.os.Message
import android.view.MotionEvent
import android.view.View
import android.view.ViewGroup
import android.webkit.RenderProcessGoneDetail
import android.webkit.WebChromeClient
import android.webkit.WebView
import android.widget.FrameLayout
import io.paritytech.polkadotapp.feature_dotns_api.presentation.DotNsWebViewClient
import io.paritytech.polkadotapp.feature_products_impl.presentation.bot.footer.web.ProductPageBridge
import io.paritytech.polkadotapp.feature_products_impl.presentation.bot.footer.web.ProductPageWebViewClient
import io.paritytech.polkadotapp.feature_products_impl.presentation.bot.footer.web.applyProductPageSettings
import io.paritytech.polkadotapp.feature_products_impl.presentation.bot.footer.web.destroyProductPage
import timber.log.Timber

/**
 * A see-through webview over the whole screen, showing one of the product's own pages for a moment (confetti after a
 * slide, say). It takes no touches, so the chat under it stays usable, and screen readers skip it.
 *
 * It goes away when the page posts `{kind:"overlayDone"}`, when [limitMillis] runs out, or when whoever showed it
 * dismisses it. Only one shows at a time on a screen: [show] refuses a second one while the first is up.
 */
@SuppressLint("ViewConstructor")
class ProductOverlayWebLayout private constructor(
    context: Context,
    private val productId: String,
    pageUrl: String,
    content: DotNsWebViewClient,
    private val limitMillis: Long,
) : FrameLayout(context) {
    private val webView = WebView(context)
    private var released = false
    private val expire = Runnable {
        Timber.d("Overlay of %s ran out of time", productId)
        dismiss()
    }

    init {
        setBackgroundColor(Color.TRANSPARENT)
        isClickable = false
        isFocusable = false
        importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS
        webView.applyProductPageSettings()
        webView.isFocusable = false
        webView.webViewClient = OverlayWebViewClient(content)
        webView.webChromeClient = OverlayChromeClient()
        ProductPageBridge.install(webView, ProductOverlayMessage.BRIDGE_NAME, productId, LABEL) { data ->
            when (ProductOverlayMessage.parse(data)) {
                // Not inside the webview's own message callback: dismiss() destroys it.
                ProductOverlayMessage.Done -> post { dismiss() }
                null -> Timber.d("Overlay of %s dropped an unreadable message", productId)
            }
        }
        addView(webView, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
        webView.loadUrl(pageUrl)
    }

    /** Whether it is still on screen. */
    val isShowing: Boolean
        get() = parent != null && !released

    /** Whether its webview has been freed, which happens once it leaves the screen for any reason. For tests. */
    internal val isReleased: Boolean
        get() = released

    /** Takes the overlay away and frees its webview. Safe to call more than once. */
    fun dismiss() {
        removeCallbacks(expire)
        (parent as? ViewGroup)?.removeView(this)
        release()
    }

    override fun onAttachedToWindow() {
        super.onAttachedToWindow()
        postDelayed(expire, limitMillis)
    }

    override fun onDetachedFromWindow() {
        removeCallbacks(expire)
        super.onDetachedFromWindow()
        // The screen can go away under the overlay (the activity finished or was recreated). Nothing calls dismiss()
        // then, so the webview is freed here, or it would keep the old activity alive.
        release()
    }

    private fun release() {
        if (released) return
        released = true
        destroyProductPage(webView)
    }

    // Every touch goes to what is under the overlay: the chat.
    @SuppressLint("ClickableViewAccessibility")
    override fun dispatchTouchEvent(event: MotionEvent): Boolean = false

    override fun onInterceptTouchEvent(event: MotionEvent): Boolean = false

    private inner class OverlayWebViewClient(content: DotNsWebViewClient) : ProductPageWebViewClient(productId, content, LABEL) {
        override fun onRenderProcessGone(view: WebView, detail: RenderProcessGoneDetail?): Boolean {
            // A moment's effect is not worth a reload: it just ends.
            post { dismiss() }
            return true
        }
    }

    private class OverlayChromeClient : WebChromeClient() {
        override fun onCreateWindow(view: WebView?, isDialog: Boolean, isUserGesture: Boolean, resultMsg: Message?): Boolean = false
    }

    companion object {
        const val OVERLAY_LIMIT_MILLIS = 5_000L
        private const val LABEL = "Overlay"

        /**
         * Shows the product page at [pageUrl] over the whole screen that [anchor] is on. Returns null, and shows
         * nothing, when that screen already shows an overlay or [anchor] is not on a screen.
         */
        fun show(
            anchor: View,
            productId: String,
            pageUrl: String,
            content: DotNsWebViewClient,
            limitMillis: Long = OVERLAY_LIMIT_MILLIS,
        ): ProductOverlayWebLayout? {
            val screen = anchor.context.findActivity()?.findViewById<ViewGroup>(android.R.id.content) ?: run {
                Timber.w("Overlay of %s has no screen to show on", productId)
                return null
            }
            if ((0 until screen.childCount).any { screen.getChildAt(it) is ProductOverlayWebLayout }) {
                Timber.d("Overlay of %s refused: one is already showing", productId)
                return null
            }
            val overlay = ProductOverlayWebLayout(anchor.context, productId, pageUrl, content, limitMillis)
            screen.addView(overlay, ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
            return overlay
        }

        private tailrec fun Context.findActivity(): Activity? = when (this) {
            is Activity -> this
            is ContextWrapper -> baseContext.findActivity()
            else -> null
        }
    }
}
