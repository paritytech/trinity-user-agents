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
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.CallingProductIdProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.PageLifecycleSource
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.UrlDerivedProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.navigation.NavigationPolicy
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import android.net.Uri
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.ProductTrUAPIHostBridge

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
    private val servingHostResolver: DotNsServingHostResolver,
    dispatchers: CoroutineDispatchers,
    @Assisted private val initialUrl: String,
    @Assisted private val navigationPolicy: NavigationPolicy,
    @Assisted private val allowIframes: Boolean,
    @Assisted private val scope: CoroutineScope,
) : WebViewProvider(dispatchers), PageLifecycleSource {
    @AssistedFactory
    interface Factory {
        fun create(
            initialUrl: String,
            navigationPolicy: NavigationPolicy,
            allowIframes: Boolean,
            scope: CoroutineScope
        ): BrowserWebViewProvider
    }

    override val callingProductIdProvider: CallingProductIdProvider = UrlDerivedProductId(dotNsTldProvider) {
        accessWebView(WebView::getUrl)
    }

    // Per-session decorator over the resolver: tracks the domain currently being served and
    // exposes its download/unpack progress for the host UI to render.
    private val contentLoader = DotNsContentLoader(dotNsResolver)
    private var nativeBridge: ProductTrUAPIHostBridge? = null

    /** Load progress of the domain the WebView is currently resolving content for. */
    val loadProgress: Flow<DotNsLoadProgress> = contentLoader.loadProgress

    private val permissionClient = webViewPermissionClientFactory.create(callingProductIdProvider, firstPartyOrigin = null)
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
                // Native Media owns capture; ordinary media playback does not require a gesture.
                mediaPlaybackRequiresUserGesture = false
            }

            val innerClient =
                BrowserWebViewClient(
                    contentLoader,
                    dotNsTldProvider,
                    servingHostResolver,
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

    fun bindTrUAPILifecycle(bridge: ProductTrUAPIHostBridge) {
        nativeBridge = bridge
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
            val initial = Uri.parse(initialUrl)
            val target = Uri.parse(url)
            if (target.scheme != initial.scheme || target.encodedAuthority != initial.encodedAuthority) {
                nativeBridge?.stop()
                nativeBridge = null
            }
            permissionClient.onPageStarted(view, url, favicon)
            innerClient.onPageStarted(view, url, favicon)
            notifyOnPageStarted(url)
        }

        override fun onPageFinished(view: WebView, url: String?) {
            innerClient.onPageFinished(view, url)
            notifyOnPageFinished()
        }

        override fun onScaleChanged(view: WebView, oldScale: Float, newScale: Float) {
            nativeBridge?.viewportChanged()
            innerClient.onScaleChanged(view, oldScale, newScale)
        }

        override fun onRenderProcessGone(view: WebView, detail: RenderProcessGoneDetail?): Boolean {
            nativeBridge?.releaseMedia()
            replaceDeadWebView(view, scope, initialUrl)
            return true
        }
    }
}
