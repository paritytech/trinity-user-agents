package io.paritytech.polkadotapp.feature_products_impl.domain.webView

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Bitmap
import android.graphics.Color
import android.view.ViewGroup
import android.webkit.RenderProcessGoneDetail
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.qualifiers.ApplicationContext
import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.presentation.DotNsContentLoader
import io.paritytech.polkadotapp.feature_dotns_api.presentation.DotNsServingHostResolver
import io.paritytech.polkadotapp.feature_products_api.model.ExecutableKind
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.CallingProductIdProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.FixedProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.PageLifecycleSource
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.UrlDerivedProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.navigation.NavigationPolicy
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow

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
        ): BrowserWebViewProvider
    }

    // A page served from a debug loopback url has no dotNS host to read its product from.
    override val callingProductIdProvider: CallingProductIdProvider =
        fixedProductId?.let(::FixedProductId) ?: UrlDerivedProductId(dotNsTldProvider) {
            accessWebView(WebView::getUrl)
        }

    // Per-session decorator over the resolver: tracks the domain currently being served and
    // exposes its download/unpack progress for the host UI to render.
    private val contentLoader = DotNsContentLoader(dotNsResolver)

    // No dotNS load reports progress for a page served from a loopback url, so its own load does.
    private val servedPageProgress = MutableStateFlow<DotNsLoadProgress>(DotNsLoadProgress.Idle)

    /** Load progress of the domain the WebView is currently resolving content for. */
    val loadProgress: Flow<DotNsLoadProgress> =
        if (fixedProductId != null) servedPageProgress else contentLoader.loadProgress

    private val permissionClient = webViewPermissionClientFactory.create(callingProductIdProvider, firstPartyOrigin)
    private val chromeClient = productWebChromeClientFactory.create(
        logPrefix = "Browser: $initialUrl",
        callingProductIdProvider = callingProductIdProvider,
        scope = scope,
        onTitleReceived = null,
    )

    @SuppressLint("SetJavaScriptEnabled")
    override suspend fun createWebView(): WebView {
        return WebView(context).apply {
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
                )
            webViewClient = InternalWebViewClient(innerClient)
            webChromeClient = chromeClient
        }
    }

    fun useTrUAPIPermissions(productId: ProductId, execution: TrUAPIProductExecution) {
        permissionClient.useTrUAPIPermissions(productId, execution, scope)
    }

    override suspend fun loadInitialContent() {
        accessWebView { it.loadUrl(initialUrl) }
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
        }

        override fun onRenderProcessGone(view: WebView, detail: RenderProcessGoneDetail?): Boolean {
            replaceDeadWebView(view, scope, initialUrl)
            return true
        }
    }
}
