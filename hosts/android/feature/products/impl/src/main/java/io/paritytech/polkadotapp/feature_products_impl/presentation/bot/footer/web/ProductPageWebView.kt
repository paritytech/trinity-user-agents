package io.paritytech.polkadotapp.feature_products_impl.presentation.bot.footer.web

import android.annotation.SuppressLint
import android.graphics.Color
import android.view.View
import android.view.ViewGroup
import android.webkit.JavascriptInterface
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature
import io.paritytech.polkadotapp.feature_dotns_api.presentation.DotNsWebViewClient
import timber.log.Timber
import java.io.ByteArrayInputStream

/**
 * Keeps a webview on one of the product's own pages, the room footer or an overlay: navigation stays on the product's
 * origin, every request is answered from the product's content and never from the network, and a missing page stays
 * missing instead of falling back to the product's SPA index.
 */
open class ProductPageWebViewClient(
    private val productId: String,
    private val content: DotNsWebViewClient,
    private val label: String,
) : WebViewClient() {
    override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
        val url = request.url.toString()
        val allowed = ProductRoomFooterWebPage.isAllowedNavigation(url, productId)
        if (!allowed) Timber.w("%s of %s refused to leave the product origin for %s", label, productId, url)
        return !allowed
    }

    override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest): WebResourceResponse? {
        val url = request.url
        if (!ProductRoomFooterWebPage.isProductOrigin(url.toString(), productId)) {
            Timber.w("%s of %s refused a request off the product origin: %s", label, productId, url)
            return refusedResponse()
        }
        if (request.isForMainFrame) {
            val archive = content.resolveHostFile(url)
            if (content.resolveFile(archive, url.path.orEmpty().removePrefix("/")) == null) return refusedResponse()
        }
        return content.shouldInterceptRequest(view, request) ?: refusedResponse()
    }

    private fun refusedResponse() = WebResourceResponse("text/plain", "UTF-8", ByteArrayInputStream(ByteArray(0))).apply {
        setStatusCodeAndReasonPhrase(404, "Not Found")
    }
}

/**
 * Gives a product page `window.<name>.postMessage(text)`, and hands [onMessage] only what the page's own main frame
 * posts from the product's origin.
 */
object ProductPageBridge {
    @SuppressLint("JavascriptInterface")
    fun install(view: WebView, name: String, productId: String, label: String, onMessage: (String) -> Unit) {
        val origin = ProductRoomFooterWebPage.origin(productId)
        val receive = { data: String?, sourceOrigin: String?, isMainFrame: Boolean ->
            when {
                !isMainFrame || !sourceOrigin?.trimEnd('/').equals(origin, ignoreCase = true) ->
                    Timber.w("%s of %s dropped a message from outside the product page", label, productId)
                data == null -> Timber.d("%s of %s dropped an empty message", label, productId)
                else -> onMessage(data)
            }
        }
        if (WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)) {
            // Injected only into documents of the product's origin; the listener also sees which frame posted.
            WebViewCompat.addWebMessageListener(view, name, setOf(origin)) { _, message, sourceOrigin, isMainFrame, _ ->
                receive(message.data, sourceOrigin?.toString(), isMainFrame)
            }
        } else {
            // No frame can host a sender here: navigation stays on the product's origin and the document's CSP refuses
            // iframes, so the posting document is the main frame, which is checked against the origin on the UI thread.
            view.addJavascriptInterface(LegacyBridge(view, productId, origin, receive), name)
        }
    }

    private class LegacyBridge(
        private val view: WebView,
        private val productId: String,
        private val origin: String,
        private val receive: (String?, String?, Boolean) -> Unit,
    ) {
        @JavascriptInterface
        fun postMessage(data: String?) {
            view.post {
                val url = view.url
                val sourceOrigin = url?.takeIf { ProductRoomFooterWebPage.isProductOrigin(it, productId) }?.let { origin }
                receive(data, sourceOrigin, true)
            }
        }
    }
}

/** Stops [view] and frees it; its client no longer hears the page. */
internal fun destroyProductPage(view: WebView) {
    view.stopLoading()
    view.webViewClient = WebViewClient()
    (view.parent as? ViewGroup)?.removeView(view)
    view.destroy()
}

/** What every product page webview in the chat shares: a transparent page that runs its own script and opens nothing. */
@SuppressLint("SetJavaScriptEnabled")
internal fun WebView.applyProductPageSettings() {
    setBackgroundColor(Color.TRANSPARENT)
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
        // The page lays its text out for the size the host gives it. The system font scale would otherwise grow the
        // text past it (two lines at 2.0x no longer fit the footer's pill), and iOS does not scale the page either.
        // Screen readers still read the page's label.
        textZoom = 100
    }
}
