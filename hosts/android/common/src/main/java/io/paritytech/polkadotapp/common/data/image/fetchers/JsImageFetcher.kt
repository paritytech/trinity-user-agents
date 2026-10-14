package io.paritytech.polkadotapp.common.data.image.fetchers

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.BitmapFactory
import android.graphics.drawable.Drawable
import android.webkit.JavascriptInterface
import android.webkit.RenderProcessGoneDetail
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.core.graphics.drawable.toDrawable
import coil.ImageLoader
import coil.decode.DataSource
import coil.fetch.DrawableResult
import coil.fetch.FetchResult
import coil.fetch.Fetcher
import coil.request.Options
import io.paritytech.polkadotapp.common.data.image.loadables.JsImageLoadable
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import timber.log.Timber
import kotlin.coroutines.resume
import kotlin.io.encoding.Base64

class JsImageFetcher(
    private val loadable: JsImageLoadable,
    private val dispatcher: CoroutineDispatcher,
    private val getWebView: () -> JSImageLoaderWebView
) : Fetcher {
    override suspend fun fetch(): FetchResult? = withContext(dispatcher) {
        loadable.getDrawable(getWebView())?.let {
            DrawableResult(
                drawable = it,
                isSampled = false,
                dataSource = DataSource.NETWORK
            )
        }
    }

    class Factory(
        private val context: Context,
        private val dispatchers: CoroutineDispatchers,
    ) : Fetcher.Factory<JsImageLoadable> {
        private val dispatcher: CoroutineDispatcher get() = dispatchers.main
        private var webView: JSImageLoaderWebView? = null

        private fun getOrCreateWebView(): JSImageLoaderWebView {
            val existing = webView
            if (existing != null && !existing.isRendererCrashed) return existing

            return JSImageLoaderWebView(context, dispatcher).also { webView = it }
        }

        override fun create(
            data: JsImageLoadable,
            options: Options,
            imageLoader: ImageLoader
        ) = JsImageFetcher(
            loadable = data,
            dispatcher = dispatcher,
            getWebView = ::getOrCreateWebView
        )
    }
}

@SuppressLint("SetJavaScriptEnabled")
class JSImageLoaderWebView(
    context: Context,
    private val dispatcher: CoroutineDispatcher,
) : WebView(context) {
    private val mutex = Mutex()
    private var callback: ((String) -> Unit)? = null

    @Volatile
    var isRendererCrashed: Boolean = false
        private set

    init {
        settings.javaScriptEnabled = true
        addJavascriptInterface(WebInterface(), "appInterface")

        webViewClient = object : WebViewClient() {
            override fun onRenderProcessGone(view: WebView?, detail: RenderProcessGoneDetail?): Boolean {
                Timber.e("Render process gone, didCrash=${detail?.didCrash()}, priority=${detail?.rendererPriorityAtExit()}")
                isRendererCrashed = true
                return true
            }
        }
    }

    suspend fun renderScriptIntoDrawable(htmlPage: String): Drawable {
        return mutex.withLock {
            withContext(dispatcher) {
                renderScriptIntoDrawableInternal(htmlPage)
            }
        }
    }

    private suspend fun renderScriptIntoDrawableInternal(htmlPage: String): Drawable {
        return suspendCancellableCoroutine { continuation ->
            onBase64Received {
                val decoded = Base64.decode(it)
                val b = BitmapFactory.decodeByteArray(decoded, 0, decoded.size)
                continuation.resume(b.toDrawable(context.resources))
            }

            loadData(htmlPage, "text/html", "base64")
        }
    }

    private fun onBase64Received(callback: (String) -> Unit) {
        this.callback = callback
    }

    private inner class WebInterface {
        @JavascriptInterface
        fun postMessage(message: String) {
            callback?.invoke(message.cleanUpBase64())
        }

        private fun String.cleanUpBase64() =
            replace("\"", "")
                .replace("data:image/png;base64,", "")
    }
}
