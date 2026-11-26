package io.paritytech.polkadotapp.feature_products_impl.presentation.exploreProducts

import androidx.core.net.toUri
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.flowOf
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.shareInBackground
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.model.productIdFromUrl
import io.paritytech.polkadotapp.feature_products_api.presentation.SpaBrowserPayload
import io.paritytech.polkadotapp.feature_products_impl.domain.exploreProducts.ExploreProductsService
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.navigation.NavigationPolicy
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductRegistrar
import io.paritytech.polkadotapp.feature_products_impl.domain.product.launchEnsureRegistered
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPISessionStarter
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.BrowserWebViewProvider
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import uniffi.truapi.ProductId
import javax.inject.Inject

/**
 * Explore catalog: a product-listing WebView driven by the Rust TrUAPI core.
 * Cross-`.dot` taps are intercepted by
 * [NavigationPolicy.CatalogNavigation] and open the selected product via the
 * router, rather than navigating inline.
 */
@HiltViewModel
class ExploreProductsViewModel @Inject constructor(
    private val browserWebViewProviderFactory: BrowserWebViewProvider.Factory,
    private val sessionStarter: TrUAPISessionStarter,
    private val productRegistrar: ProductRegistrar,
    private val router: ProductsRouter,
    private val exploreProductsService: ExploreProductsService,
    private val dotNsTldProvider: DotNsTldProvider,
) : BaseViewModel() {
    private val providerFlow = flowOf {
        createProvider()
    }.filterNotNull().shareInBackground()

    @OptIn(ExperimentalCoroutinesApi::class)
    val webViewFlow = providerFlow
        .flatMapLatest { it.webViews() }
        .stateIn(scope = this, started = SharingStarted.Eagerly, initialValue = null)

    fun onProductSelected(productId: ProductId) {
        launch { router.openSpaBrowser(SpaBrowserPayload.ByProductId(productId)) }
    }

    fun pauseConnections() {
        launch { providerFlow.first().pauseConnections() }
    }

    fun resumeConnections() {
        launch { providerFlow.first().resumeConnections() }
    }

    private suspend fun createProvider(): BrowserWebViewProvider? {
        val exploreUrl = exploreProductsService.getExploreUrl()
            .logFailure("Failed to resolve explore url")
            .getOrNull() ?: return null

        val navigationPolicy = NavigationPolicy.CatalogNavigation(::onProductSelected, dotNsTldProvider)

        val webViewProvider = browserWebViewProviderFactory.create(exploreUrl, navigationPolicy, allowIframes = false, viewModelScope)
        webViewProvider.addOnPageStartedListener { url ->
            dotNsTldProvider.currentTldOrNull()
                ?.let { tld -> productIdFromUrl(url.toUri(), tld).getOrNull() }
                ?.let { productRegistrar.launchEnsureRegistered(it) }
        }

        // Explore has no deeplink handler of its own, so leaving the catalog opens
        // the product in the SPA browser, matching NavigationPolicy.CatalogNavigation.
        val hostApiNavigation = NavigationPolicy.HostApiNavigation(
            onDeeplinkNavigation = { destination ->
                dotNsTldProvider.currentTldOrNull()
                    ?.let { tld -> productIdFromUrl(destination, tld) }
                    ?.onSuccess(::onProductSelected)
                    ?.logFailure("Explore navigateTo: not a product url $destination")
            },
            webViewLoader = { target -> viewModelScope.launch { webViewProvider.getWebView().loadUrl(target) } },
            dotNsTldProvider = dotNsTldProvider,
        )

        sessionStarter.start(webViewProvider, exploreUrl, viewModelScope, hostApiNavigation)
        return webViewProvider
    }
}
