package io.paritytech.polkadotapp.feature_products_impl.domain.browser

import android.net.Uri
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.navigation.NavigationPolicy
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPISessionStarter
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.BrowserWebViewProvider
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import javax.inject.Inject

/**
 * Builds the live runtime for a single browser tab — the WebView provider plus
 * its Rust TrUAPI host bridge — all hosted in the tab's [CoroutineScope]
 * (cancelling it disposes the core and destroys the WebView).
 *
 * [RealProductSessionController] just attaches its listeners to the returned provider.
 */
class TrUAPIProductTabSessionFactory @Inject constructor(
    private val browserWebViewProviderFactory: BrowserWebViewProvider.Factory,
    private val sessionStarter: TrUAPISessionStarter,
    private val dotNsTldProvider: DotNsTldProvider,
) {
    fun create(url: String, scope: CoroutineScope, onDeeplink: (Uri) -> Unit): BrowserWebViewProvider {
        val provider = browserWebViewProviderFactory.create(
            url,
            NavigationPolicy.InlineNavigation(onDeeplinkNavigation = onDeeplink),
            allowIframes = true,
            scope,
        )

        val hostApiNavigation = NavigationPolicy.HostApiNavigation(
            onDeeplinkNavigation = onDeeplink,
            webViewLoader = { target -> scope.launch { provider.getWebView().loadUrl(target) } },
            dotNsTldProvider = dotNsTldProvider,
        )

        sessionStarter.start(provider, url, scope, hostApiNavigation)
        return provider
    }
}
