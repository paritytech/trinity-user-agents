package io.paritytech.polkadotapp.feature_products_impl.domain.webView

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Bitmap
import android.graphics.Color
import android.view.View
import android.view.ViewGroup
import android.webkit.RenderProcessGoneDetail
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.qualifiers.ApplicationContext
import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.common.data.telemetry.HostLoadContext
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.presentation.DotNsContentLoader
import io.paritytech.polkadotapp.feature_dotns_api.presentation.DotNsServingHostResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.CallingProductIdProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.FixedProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.PageLifecycleSource
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.UrlDerivedProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.navigation.NavigationPolicy
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import uniffi.truapi.ExecutableKind
import uniffi.truapi.ProductId
import java.util.concurrent.CopyOnWriteArrayList
import kotlin.coroutines.EmptyCoroutineContext
import kotlin.time.Duration.Companion.seconds

/**
 * Visible WebView provider for SPA browser and Explore environments.
 *
 * Handles WebView lifecycle, DotNs content serving, permission checks, and navigation.
 * Does NOT inject scripts — that responsibility belongs to [PageLoadInjection]
 * which hooks into this provider via [PageLifecycleSource].
 *
 * Replaces [SpaProductWebViewProvider] and [SpaExplorerWebViewProvider].
 */
class BrowserWebViewProvider @AssistedInject constructor(
    @param:ApplicationContext private val context: Context,
    private val productWebChromeClientFactory: ProductWebChromeClient.Factory,
    private val webViewPermissionClientFactory: WebViewPermissionClientFactory,
    private val dotNsResolver: DotNsResolver,
    private val dotNsTldProvider: DotNsTldProvider,
    private val servingHostResolver: ProductServingHostResolver,
    dispatchers: CoroutineDispatchers,
    @Assisted("initialUrl") private val initialUrl: String,
    @Assisted private val navigationPolicy: NavigationPolicy,
    @Assisted private val allowIframes: Boolean,
    @Assisted private val scope: CoroutineScope,
    @Assisted private val fixedProductId: ProductId?,
    @Assisted("firstPartyOrigin") private val firstPartyOrigin: String?,
    @Assisted private val servedExecutable: ExecutableKind,
    @Assisted("waitForInitialTheme") private val waitForInitialTheme: Boolean,
) : WebViewProvider(dispatchers), PageLifecycleSource {
    @AssistedFactory
    interface Factory {
        fun create(
            @Assisted("initialUrl") initialUrl: String,
            navigationPolicy: NavigationPolicy,
            allowIframes: Boolean,
            scope: CoroutineScope,
            fixedProductId: ProductId? = null,
            @Assisted("firstPartyOrigin") firstPartyOrigin: String? = null,
            servedExecutable: ExecutableKind = ExecutableKind.APP,
            @Assisted("waitForInitialTheme") waitForInitialTheme: Boolean = false,
        ): BrowserWebViewProvider
    }

    // A page served from a debug loopback url has no dotNS host to read its product from. The id stays fixed across
    // navigations, so a debug page that navigates to another product's page runs that page as the debug product.
    override val callingProductIdProvider: CallingProductIdProvider =
        fixedProductId?.let(::FixedProductId) ?: UrlDerivedProductId(dotNsTldProvider) {
            accessWebView(WebView::getUrl)
        }

    // Per-session decorator over the resolver: tracks the domain currently being served and
    // exposes its download/unpack progress for the host UI to render.
    private val contentLoader = DotNsContentLoader(dotNsResolver)

    // Set once the page's host session is known, before the page loads; the archive lookups behind
    // its requests report under it.
    @Volatile
    private var hostLoad: HostLoadContext? = null

    private val onRevealedListeners = CopyOnWriteArrayList<(PageReveal) -> Unit>()

    // No dotNS load reports progress for a page served from a loopback url, so its own load does.
    private val servedPageProgress = MutableStateFlow<DotNsLoadProgress>(DotNsLoadProgress.Idle)

    /** Load progress of the domain the WebView is currently resolving content for. */
    val loadProgress: Flow<DotNsLoadProgress> =
        if (fixedProductId != null) servedPageProgress else contentLoader.loadProgress

    val isPageRevealed = MutableStateFlow(!waitForInitialTheme)

    private val permissionClient = webViewPermissionClientFactory.create(callingProductIdProvider, firstPartyOrigin)
    private val chromeClient = productWebChromeClientFactory.create(
        logPrefix = "Browser: $initialUrl",
        callingProductIdProvider = callingProductIdProvider,
        scope = scope,
        onTitleReceived = null,
    )

    @SuppressLint("SetJavaScriptEnabled", "ClickableViewAccessibility")
    override suspend fun createWebView(): WebView {
        return WebView(context).apply {
            setBackgroundColor(Color.TRANSPARENT)
            if (waitForInitialTheme) {
                alpha = 0f
                isPageRevealed.value = false
                // WebView takes touches even when disabled, so a hidden page is shielded by consuming them.
                setOnTouchListener { _, _ -> true }
                isFocusable = false
                isFocusableInTouchMode = false
                importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO_HIDE_DESCENDANTS
            }
            // This is needed so webView has a proper initial viewport height when the page renders. With overflow: hidden and height: 100%, if the WebView's layout height isn't resolved yet, the
            //  content box collapses to 0px
            layoutParams = ViewGroup.LayoutParams(
                ViewGroup.LayoutParams.MATCH_PARENT,
                ViewGroup.LayoutParams.MATCH_PARENT
            )
            // A WebView paints white before its first frame. Left opaque it flashes against the
            // host around it, which is at its worst under an expanding card.
            setBackgroundColor(Color.TRANSPARENT)
            settings.apply {
                javaScriptEnabled = true
                domStorageEnabled = true
                allowFileAccess = false
                allowContentAccess = false
                // A camera preview is a MediaStream in an autoplaying <video>; the default gesture
                // requirement would keep it black even once the capture permission is granted.
                mediaPlaybackRequiresUserGesture = false
            }

            val innerClient =
                BrowserWebViewClient(
                    contentLoader,
                    dotNsTldProvider,
                    DotNsServingHostResolver { host -> servingHostResolver.servingHostFor(host, servedExecutable) },
                    navigationPolicy,
                    frameEmbeddingResponseHeaders(allowIframes),
                    firstPartyOrigin,
                    loadContext = { hostLoad ?: EmptyCoroutineContext },
                )
            webViewClient = InternalWebViewClient(innerClient)
            webChromeClient = chromeClient
        }
    }

    fun useHostLoad(load: HostLoadContext) {
        hostLoad = load
    }

    /** Called once a page held hidden for its theme is shown, saying what showed it. */
    fun addOnRevealedListener(listener: (PageReveal) -> Unit) {
        onRevealedListeners += listener
    }

    fun useTrUAPIPermissions(productId: ProductId, execution: TrUAPIProductExecution) {
        permissionClient.useTrUAPIPermissions(productId, execution, scope)
    }

    override suspend fun loadInitialContent() {
        accessWebView { it.loadUrl(initialUrl) }
    }

    override fun onInitialThemeDelivered() {
        if (!waitForInitialTheme) return
        getWebViewOrNull()?.let { view -> view.post { revealAfterRendering(view) } }
    }

    private fun revealAfterRendering(view: WebView) {
        if (getWebViewOrNull() !== view || isPageRevealed.value) return
        view.postVisualStateCallback(0L, object : WebView.VisualStateCallback() {
            override fun onComplete(requestId: Long) = reveal(view, PageReveal.THEME)
        })
    }

    private fun reveal(view: WebView, via: PageReveal) {
        if (getWebViewOrNull() !== view || isPageRevealed.value) return
        view.alpha = 1f
        isPageRevealed.value = true
        view.setOnTouchListener(null)
        view.isFocusable = true
        view.isFocusableInTouchMode = true
        view.importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_AUTO
        onRevealedListeners.forEach { it(via) }
    }

    private inner class InternalWebViewClient(
        private val innerClient: WebViewClient,
    ) : WebViewClient() {
        override fun shouldInterceptRequest(
            view: WebView,
            request: WebResourceRequest,
        ): WebResourceResponse? {
            return permissionClient.shouldInterceptRequest(view, request)
                ?: innerClient.shouldInterceptRequest(view, request)
        }

        override fun shouldOverrideUrlLoading(view: WebView?, request: WebResourceRequest?): Boolean {
            return innerClient.shouldOverrideUrlLoading(view, request)
        }

        override fun onPageStarted(view: WebView, url: String, favicon: Bitmap?) {
            permissionClient.onPageStarted(view, url, favicon)
            innerClient.onPageStarted(view, url, favicon)
            notifyOnPageStarted(url)
        }

        override fun onPageFinished(view: WebView, url: String?) {
            innerClient.onPageFinished(view, url)
            if (fixedProductId != null) servedPageProgress.value = DotNsLoadProgress.Completed
            notifyOnPageFinished()
            if (waitForInitialTheme) {
                view.postDelayed({ reveal(view, PageReveal.FALLBACK) }, THEME_READY_FALLBACK.inWholeMilliseconds)
            }
        }

        override fun onReceivedHttpError(
            view: WebView,
            request: WebResourceRequest,
            errorResponse: WebResourceResponse,
        ) {
            if (request.isForMainFrame) notifyMainFrameFailed()
        }

        // An aborted load, such as one a newer navigation replaced, arrives as ERROR_UNKNOWN.
        override fun onReceivedError(view: WebView, request: WebResourceRequest, error: WebResourceError) {
            if (request.isForMainFrame && error.errorCode != WebViewClient.ERROR_UNKNOWN) notifyMainFrameFailed()
        }

        override fun onRenderProcessGone(view: WebView, detail: RenderProcessGoneDetail?): Boolean {
            replaceDeadWebView(view, scope, initialUrl)
            return true
        }
    }

    private companion object {
        val THEME_READY_FALLBACK = 2.seconds
    }
}

/** What showed a page held hidden for its theme: the theme arriving, or the timer that gives up on it. */
enum class PageReveal(val tag: String) {
    THEME("theme"),
    FALLBACK("fallback"),
}
