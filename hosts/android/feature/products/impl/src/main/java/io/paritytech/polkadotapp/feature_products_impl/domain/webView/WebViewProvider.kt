package io.paritytech.polkadotapp.feature_products_impl.domain.webView

import android.view.ViewGroup
import android.webkit.WebView
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.CallingProductIdProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.PageLifecycleSource
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

abstract class WebViewProvider(
    private val dispatchers: CoroutineDispatchers
) : PageLifecycleSource {
    private val mutex = Mutex()
    private val cachedWebView = MutableStateFlow<WebView?>(null)
    private val webViewSetups = mutableListOf<(WebView) -> Unit>()

    private val onPageFinishedListeners = mutableListOf<() -> Unit>()
    private val onPageStartedListeners = mutableListOf<(String) -> Unit>()
    private val onWebViewDestroyedListeners = mutableListOf<() -> Unit>()

    abstract val callingProductIdProvider: CallingProductIdProvider

    protected abstract suspend fun createWebView(): WebView

    /**
     * Loads the initial content into the WebView.
     * All data needed (URL, HTML, scripts) is supplied at construction time via the factory.
     */
    abstract suspend fun loadInitialContent()

    suspend fun getWebView(): WebView {
        return mutex.withLock {
            cachedWebView.value?.let { return it }

            withContext(dispatchers.main) {
                createWebView().also { webView -> webViewSetups.forEach { it(webView) } }
            }
                .also { cachedWebView.value = it }
        }
    }

    /** The WebView to show, created on first collection; null while a dead one awaits its replacement. */
    fun webViews(): Flow<WebView?> = flow {
        getWebView()
        emitAll(cachedWebView)
    }

    /** Runs [setup] on the current WebView and on every WebView created after it. */
    suspend fun addWebViewSetup(setup: (WebView) -> Unit) {
        withContext(dispatchers.main) {
            mutex.withLock {
                webViewSetups += setup
                cachedWebView.value?.let(setup)
            }
        }
    }

    suspend fun <R> accessWebView(action: suspend (WebView) -> R): R {
        return withContext(dispatchers.main) {
            val webView = getWebView()
            action(webView)
        }
    }

    fun getWebViewOrNull(): WebView? = cachedWebView.value

    fun addOnWebViewDestroyedListener(listener: () -> Unit) {
        onWebViewDestroyedListeners += listener
    }

    override fun addOnPageFinishedListener(listener: () -> Unit) {
        onPageFinishedListeners += listener
    }

    override fun addOnPageStartedListener(listener: (url: String) -> Unit) {
        onPageStartedListeners += listener
    }

    /** Detaches and destroys [dead] after its renderer is gone: Android never lets it render again. */
    protected fun discardDeadWebView(dead: WebView) {
        (dead.parent as? ViewGroup)?.removeView(dead)
        dead.destroy()
        cachedWebView.compareAndSet(dead, null)
        onWebViewDestroyedListeners.forEach { it.invoke() }
    }

    /**
     * Discards [dead] and loads the page it showed, or [fallbackUrl], in a fresh WebView that every
     * setup has run on again.
     */
    protected fun replaceDeadWebView(dead: WebView, scope: CoroutineScope, fallbackUrl: String) {
        val url = dead.url ?: fallbackUrl
        discardDeadWebView(dead)
        scope.launch { accessWebView { it.loadUrl(url) } }
    }

    protected fun notifyOnPageFinished() {
        onPageFinishedListeners.forEach { it.invoke() }
    }

    protected fun notifyOnPageStarted(url: String) {
        onPageStartedListeners.forEach { it.invoke(url) }
    }

    fun pauseConnections() {
        cachedWebView.value?.evaluateJavascript("window.__pauseConnections__?.()") {}
    }

    fun resumeConnections() {
        cachedWebView.value?.evaluateJavascript("window.__resumeConnections__?.()") {}
    }
}
