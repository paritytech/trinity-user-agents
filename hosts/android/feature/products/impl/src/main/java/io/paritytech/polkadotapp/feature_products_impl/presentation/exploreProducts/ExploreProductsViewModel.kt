package io.paritytech.polkadotapp.feature_products_impl.presentation.exploreProducts

import androidx.core.net.toUri
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.deeplink.asWebUri
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.flowOf
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.shareInBackground
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.ProductRuntimeSettings
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.presentation.SpaBrowserPayload
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductsBotApiImpl
import io.paritytech.polkadotapp.feature_products_impl.domain.exploreProducts.ExploreProductsService
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.HostApiEnvironment
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.HostApiSession
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.HostCallGroupFactory
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.PageLoadInjection
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.navigation.NavigationPolicy
import io.paritytech.polkadotapp.feature_products_impl.domain.jsRuntime.WebViewRuntime
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductRegistrar
import io.paritytech.polkadotapp.feature_products_impl.domain.product.launchEnsureRegistered
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.ProductTrUAPIHostBridge
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
import javax.inject.Inject

/**
 * Explore catalog: a product-listing WebView driven by the host runtime
 * selected via [ProductRuntimeSettings], either the native JS-bridge host or
 * the Rust TrUAPI core. Cross-`.dot` taps are intercepted by
 * [NavigationPolicy.CatalogNavigation] and open the selected product via the
 * router, rather than navigating inline.
 */
@HiltViewModel
class ExploreProductsViewModel @Inject constructor(
    private val browserWebViewProviderFactory: BrowserWebViewProvider.Factory,
    private val hostCallGroupFactory: HostCallGroupFactory,
    private val sessionFactory: HostApiSession.Factory,
    private val botApiFactory: ProductsBotApiImpl.Factory,
    private val sessionStarter: TrUAPISessionStarter,
    private val productRegistrar: ProductRegistrar,
    private val router: ProductsRouter,
    private val exploreProductsService: ExploreProductsService,
    private val runtimeSettings: ProductRuntimeSettings,
    private val dotNsTldProvider: DotNsTldProvider,
) : BaseViewModel() {
    private sealed interface SessionComponents {
        val provider: BrowserWebViewProvider

        class Native(
            override val provider: BrowserWebViewProvider,
            val session: HostApiSession,
        ) : SessionComponents

        class TrUAPI(
            override val provider: BrowserWebViewProvider,
            val bridge: ProductTrUAPIHostBridge,
        ) : SessionComponents
    }

    private val componentsFlow = flowOf {
        createComponents()
    }.filterNotNull().shareInBackground()

    @OptIn(ExperimentalCoroutinesApi::class)
    val webViewFlow = componentsFlow
        .flatMapLatest { it.provider.webViews() }
        .stateIn(scope = this, started = SharingStarted.Eagerly, initialValue = null)

    fun onProductSelected(productId: ProductId) {
        launch { router.openSpaBrowser(SpaBrowserPayload.ByProductId(productId.value)) }
    }

    fun pauseConnections() {
        launch { componentsFlow.first().provider.pauseConnections() }
    }

    fun resumeConnections() {
        launch { componentsFlow.first().provider.resumeConnections() }
    }

    private suspend fun createComponents(): SessionComponents? {
        val exploreUrl = exploreProductsService.getExploreUrl()
            .logFailure("Failed to resolve explore url")
            .getOrNull() ?: return null

        val navigationPolicy = NavigationPolicy.CatalogNavigation(::onProductSelected, dotNsTldProvider)

        val webViewProvider = browserWebViewProviderFactory.create(exploreUrl, navigationPolicy, allowIframes = false, viewModelScope)
        webViewProvider.addOnPageStartedListener { url ->
            dotNsTldProvider.currentTldOrNull()
                ?.let { tld -> ProductId.fromUrl(url.toUri(), tld).getOrNull() }
                ?.let { productRegistrar.launchEnsureRegistered(it) }
        }

        return if (runtimeSettings.isTrUAPIRuntimeEnabled()) {
            createTrUAPIComponents(webViewProvider, exploreUrl)
        } else {
            createNativeComponents(webViewProvider, navigationPolicy)
        }
    }

    private fun createNativeComponents(
        webViewProvider: BrowserWebViewProvider,
        navigationPolicy: NavigationPolicy,
    ): SessionComponents {
        val productIdProvider = webViewProvider.callingProductIdProvider
        val runtime = WebViewRuntime(webViewProvider)

        val botApi = botApiFactory.create(productIdProvider)
        val handlerGroups = hostCallGroupFactory.createShared(botApi, productIdProvider, navigationPolicy)

        val environment = HostApiEnvironment(
            injectionStrategy = PageLoadInjection(
                pageLifecycleSource = webViewProvider,
                coroutineScope = this,
            ),
            handlerGroups = handlerGroups,
        )

        val transport = runtime.createTransport()
        val session = sessionFactory.create(environment, runtime, transport, this)

        launch {
            runCatching { session.initialize() }
                .logFailure("Failed to initialize Explore host session")
        }

        return SessionComponents.Native(webViewProvider, session)
    }

    private fun createTrUAPIComponents(
        webViewProvider: BrowserWebViewProvider,
        exploreUrl: String,
    ): SessionComponents {
        // Explore has no deeplink handler of its own, so leaving the catalog opens
        // the product in the SPA browser, matching NavigationPolicy.CatalogNavigation.
        val hostApiNavigation = NavigationPolicy.HostApiNavigation(
            onDeeplinkNavigation = { destination ->
                dotNsTldProvider.currentTldOrNull()
                    ?.let { tld -> ProductId.fromUrl(destination.asWebUri(), tld) }
                    ?.onSuccess(::onProductSelected)
                    ?.logFailure("Explore navigateTo: not a product url $destination")
            },
            webViewLoader = { target -> viewModelScope.launch { webViewProvider.getWebView().loadUrl(target) } },
            dotNsTldProvider = dotNsTldProvider,
        )

        val bridge = sessionStarter.start(webViewProvider, exploreUrl, viewModelScope, hostApiNavigation)
        return SessionComponents.TrUAPI(webViewProvider, bridge)
    }
}
