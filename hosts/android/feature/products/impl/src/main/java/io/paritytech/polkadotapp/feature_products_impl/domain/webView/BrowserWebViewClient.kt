package io.paritytech.polkadotapp.feature_products_impl.domain.webView

import android.webkit.WebResourceRequest
import android.webkit.WebView
import androidx.core.net.toUri
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsNavigationType
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsUtils
import io.paritytech.polkadotapp.feature_dotns_api.presentation.DotNsServingHostResolver
import io.paritytech.polkadotapp.feature_dotns_api.presentation.DotNsWebViewClient
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.navigation.NavigationPolicy
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.navigation.NavigationResult

/**
 * WebViewClient for browser environments (SPA + Explore).
 *
 * Serves dotNS domains from local storage via [DotNsWebViewClient].
 * Delegates navigation decisions to [NavigationPolicy], passing a pre-classified [DotNsNavigationType].
 */
class BrowserWebViewClient(
    dotNsResolver: DotNsResolver,
    private val dotNsTldProvider: DotNsTldProvider,
    servingHostResolver: DotNsServingHostResolver,
    private val navigationPolicy: NavigationPolicy,
    mainDocumentResponseHeaders: Map<String, String>,
) : DotNsWebViewClient(dotNsResolver, dotNsTldProvider, servingHostResolver, mainDocumentResponseHeaders) {
    override fun shouldOverrideUrlLoading(view: WebView, request: WebResourceRequest): Boolean {
        val origin = view.url?.toUri()
        val destination = request.url
        val tld = dotNsTldProvider.currentTldOrNull()
        val type = if (tld == null) {
            DotNsNavigationType.EXTERNAL
        } else {
            DotNsUtils.classifyNavigation(origin, destination, tld)
        }
        val result = navigationPolicy.handleNavigation(type, destination)

        return when (result) {
            NavigationResult.INTERCEPTED_BY_POLICY -> true
            NavigationResult.DELEGATE_TO_WEBVIEW -> false
        }
    }
}
