package io.paritytech.polkadotapp.feature_products_impl.presentation.spaHost

import android.content.Context
import android.net.Uri
import android.webkit.WebView
import androidx.core.net.toUri
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler
import io.paritytech.polkadotapp.common.presentation.deeplink.handleAndProcessOutcomeWithSystemFallback
import io.paritytech.polkadotapp.common.presentation.screens.MessageDisplay
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.SpaHost
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.SpaHostSession
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductsBotApiImpl
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.HostApiEnvironment
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.HostApiSession
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.HostCallGroupFactory
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.PageLoadInjection
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.navigation.NavigationPolicy
import io.paritytech.polkadotapp.feature_products_impl.domain.jsRuntime.WebViewRuntime
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductRegistrar
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.BrowserWebViewProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorkerRefCounter
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.withWorkerAcquired
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import javax.inject.Inject
import javax.inject.Singleton

/** SPA host backed by the native JS-bridge host API. */
@Singleton
class NativeSpaHost @Inject constructor(
    private val browserWebViewProviderFactory: BrowserWebViewProvider.Factory,
    private val hostCallGroupFactory: HostCallGroupFactory,
    private val sessionFactory: HostApiSession.Factory,
    private val botApiFactory: ProductsBotApiImpl.Factory,
    private val productRegistrar: ProductRegistrar,
    private val deepLinkHandler: DeepLinkHandler,
    private val dotNsTldProvider: DotNsTldProvider,
    private val workerRefCounter: ProductWorkerRefCounter,
    @param:ApplicationContext private val context: Context,
) : SpaHost {
    context(scope: ComputationalScope, messageDisplay: MessageDisplay)
    override fun createSession(initialUrl: String, underCard: Boolean): SpaHostSession {
        lateinit var webViewProvider: BrowserWebViewProvider

        val webViewNavigation = NavigationPolicy.InlineNavigation(
            onDeeplinkNavigation = { launchDeeplinkNavigation(it) }
        )
        val hostApiNavigation = NavigationPolicy.HostApiNavigation(
            onDeeplinkNavigation = { launchDeeplinkNavigation(it) },
            webViewLoader = { scope.launch { webViewProvider.getWebView().loadUrl(it) } },
            dotNsTldProvider = dotNsTldProvider
        )

        webViewProvider = browserWebViewProviderFactory.create(
            initialUrl = initialUrl,
            navigationPolicy = webViewNavigation,
            allowIframes = true,
            scope = scope
        )

        val callingProductIdProvider = webViewProvider.callingProductIdProvider
        val botApi = botApiFactory.create(callingProductIdProvider)
        val runtime = WebViewRuntime(webViewProvider)
        val transport = runtime.createTransport()
        val handlerGroups = hostCallGroupFactory.createShared(
            botApi = botApi,
            productIdProvider = callingProductIdProvider,
            navigationPolicy = hostApiNavigation
        )

        val environment = HostApiEnvironment(
            injectionStrategy = PageLoadInjection(
                pageLifecycleSource = webViewProvider,
                coroutineScope = scope,
            ),
            handlerGroups = handlerGroups,
        )

        // Keep the product's worker alive for as long as the session is hosted. A product that
        // publishes no worker is a no-op; the reference releases when the session scope ends.
        scope.launch {
            val tld = dotNsTldProvider.getTld().getOrNull() ?: return@launch
            val productId = ProductId.fromUrl(initialUrl.toUri(), tld).getOrNull() ?: return@launch
            workerRefCounter.withWorkerAcquired(productId, "spa:${productId.value}") {
                awaitCancellation()
            }
        }

        val session = sessionFactory.create(environment, runtime, transport, scope)
        scope.launch {
            runCatching { session.initialize() }
                .logFailure("Failed to initialize SPA host session")
        }

        val currentUrlFlow = MutableStateFlow(initialUrl)
        webViewProvider.addOnPageStartedListener { url ->
            currentUrlFlow.value = url
            scope.launch {
                val tld = dotNsTldProvider.getTld().getOrNull() ?: return@launch
                ProductId.fromUrl(url.toUri(), tld).getOrNull()?.let {
                    productRegistrar.ensureRegistered(it)
                }
            }
        }

        val webViewFlow: StateFlow<WebView?> = webViewProvider.webViews()
            .stateIn(scope, SharingStarted.Eagerly, null)

        val loadProgressFlow: StateFlow<DotNsLoadProgress> = webViewProvider.loadProgress
            .stateIn(scope, SharingStarted.Eagerly, DotNsLoadProgress.Idle)

        val titleFlow = MutableStateFlow("")
        webViewProvider.addOnPageFinishedListener {
            titleFlow.value = webViewProvider.getWebViewOrNull()?.title.orEmpty()
        }

        return NativeSpaHostSession(webViewFlow, currentUrlFlow, loadProgressFlow, titleFlow, webViewProvider)
    }

    context(scope: ComputationalScope, messageDisplay: MessageDisplay)
    private fun launchDeeplinkNavigation(data: Uri) {
        scope.launch {
            with(context) {
                deepLinkHandler.handleAndProcessOutcomeWithSystemFallback(data)
            }
        }
    }
}

private class NativeSpaHostSession(
    override val webView: StateFlow<WebView?>,
    override val currentUrl: StateFlow<String>,
    override val loadProgress: StateFlow<DotNsLoadProgress>,
    override val title: StateFlow<String>,
    private val provider: BrowserWebViewProvider,
) : SpaHostSession {
    override fun pauseConnections() {
        provider.pauseConnections()
    }

    override fun resumeConnections() {
        provider.resumeConnections()
    }
}
