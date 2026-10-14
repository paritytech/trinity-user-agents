package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.navigation

import android.net.Uri
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsNavigationType
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsUtils
import io.paritytech.polkadotapp.feature_products_api.model.ProductId

enum class NavigationResult { INTERCEPTED_BY_POLICY, DELEGATE_TO_WEBVIEW }

/**
 * Determines how navigation requests are handled per environment.
 *
 * Receives a pre-classified [DotNsNavigationType] from the WebViewClient caller.
 * Each implementation only handles the dispatch logic, not the classification.
 *
 * [InlineNavigation] and [HostApiNavigation] look alike but cover different
 * sources. Inline is WebView-initiated, so a same-domain hop can be handed back
 * to the WebView that raised it. Host API is the product calling `navigateTo`,
 * where there is no navigation event to delegate to and the load has to be
 * issued manually. Both runtimes install the same pair.
 */
sealed interface NavigationPolicy {
    fun handleNavigation(type: DotNsNavigationType, destination: Uri): NavigationResult

    /**
     * Chat: there is no visible WebView to load into, so every navigation goes to the deeplink handler.
     */
    class DeeplinkNavigation(
        private val onDeeplinkNavigation: (Uri) -> Unit,
    ) : NavigationPolicy {
        override fun handleNavigation(type: DotNsNavigationType, destination: Uri): NavigationResult {
            onDeeplinkNavigation(destination)
            return NavigationResult.INTERCEPTED_BY_POLICY
        }
    }

    /**
     * SPA Browser: load all .dot navigation in the same WebView. External is delegated to upper layer
     */
    class InlineNavigation(
        private val onDeeplinkNavigation: (Uri) -> Unit,
    ) : NavigationPolicy {
        override fun handleNavigation(type: DotNsNavigationType, destination: Uri): NavigationResult {
            return when (type) {
                // TODO we navigate CROSS_DOTNS_DOMAIN to onExternalNavigation because
                // we want deeplink handler to open a separate SPA screen for a new product
                // This is NOT optimal since it creates a stack of WebViews, but provides "expected"
                // way of back navigation, where previous page stays at the place where user left it
                // Implementing similar behavior within the same webview is not trivial but should be done later
                DotNsNavigationType.EXTERNAL,
                DotNsNavigationType.CROSS_DOTNS_DOMAIN -> {
                    onDeeplinkNavigation(destination)
                    NavigationResult.INTERCEPTED_BY_POLICY
                }

                DotNsNavigationType.SAME_DOTNS_DOMAIN -> NavigationResult.DELEGATE_TO_WEBVIEW
            }
        }
    }

    /**
     * hostApi.navigateTo: handle loads manually
     */
    class HostApiNavigation(
        private val onDeeplinkNavigation: (Uri) -> Unit,
        private val webViewLoader: (String) -> Unit,
        private val dotNsTldProvider: DotNsTldProvider
    ) : NavigationPolicy {
        override fun handleNavigation(
            type: DotNsNavigationType,
            destination: Uri
        ): NavigationResult {
            return when (type) {
                DotNsNavigationType.EXTERNAL,
                DotNsNavigationType.CROSS_DOTNS_DOMAIN -> {
                    onDeeplinkNavigation(destination)
                    NavigationResult.INTERCEPTED_BY_POLICY
                }

                DotNsNavigationType.SAME_DOTNS_DOMAIN -> {
                    val normalized = dotNsTldProvider.currentTldOrNull()
                        ?.let { DotNsUtils.normalize(destination, it) }
                        ?: destination
                    webViewLoader(normalized.toString())
                    NavigationResult.INTERCEPTED_BY_POLICY
                }
            }
        }
    }

    /**
     * Explore: same-domain stays in WebView, cross-domain opens SPA browser.
     */
    class CatalogNavigation(
        private val onProductSelected: (ProductId) -> Unit,
        private val dotNsTldProvider: DotNsTldProvider,
    ) : NavigationPolicy {
        override fun handleNavigation(type: DotNsNavigationType, destination: Uri): NavigationResult {
            return when (type) {
                DotNsNavigationType.SAME_DOTNS_DOMAIN -> NavigationResult.DELEGATE_TO_WEBVIEW

                DotNsNavigationType.CROSS_DOTNS_DOMAIN -> {
                    val productId = dotNsTldProvider.currentTldOrNull()
                        ?.let { ProductId.fromUrl(destination, it).getOrNull() }
                        ?: return NavigationResult.DELEGATE_TO_WEBVIEW
                    onProductSelected(productId)
                    NavigationResult.INTERCEPTED_BY_POLICY
                }

                DotNsNavigationType.EXTERNAL -> NavigationResult.DELEGATE_TO_WEBVIEW
            }
        }
    }
}
