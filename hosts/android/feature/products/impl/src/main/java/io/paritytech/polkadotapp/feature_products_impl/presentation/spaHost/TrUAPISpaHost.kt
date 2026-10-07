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
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.FaceShownRequest
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.SpaHost
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.SpaHostSession
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.navigation.NavigationPolicy
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.DebugPocketCards
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.debugServedUrl
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductRegistrar
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.ProductTrUAPIHostBridge
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPISessionStarter
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.BrowserWebViewProvider
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import uniffi.truapi.ProductExecutionKind
import java.net.URI
import javax.inject.Inject
import javax.inject.Singleton

/** SPA host backed by the Rust TrUAPI runtime ([ProductTrUAPIHostBridge]). */
@Singleton
class TrUAPISpaHost @Inject constructor(
    private val browserWebViewProviderFactory: BrowserWebViewProvider.Factory,
    private val sessionStarter: TrUAPISessionStarter,
    private val productRegistrar: ProductRegistrar,
    private val deepLinkHandler: DeepLinkHandler,
    @param:ApplicationContext private val context: Context,
    private val dotNsTldProvider: DotNsTldProvider,
    private val debugPocketCards: DebugPocketCards,
) : SpaHost {
    context(scope: ComputationalScope, messageDisplay: MessageDisplay)
    override fun createSession(initialUrl: String, underCard: Boolean): SpaHostSession {
        lateinit var webViewProvider: BrowserWebViewProvider

        val servedPage = debugServedPage(initialUrl)
        val pageUrl = servedPage?.url ?: initialUrl

        val webViewNavigation = NavigationPolicy.InlineNavigation(
            onDeeplinkNavigation = { launchDeeplinkNavigation(it) }
        )

        webViewProvider = browserWebViewProviderFactory.create(
            initialUrl = pageUrl,
            navigationPolicy = webViewNavigation,
            allowIframes = true,
            scope = scope,
            fixedProductId = servedPage?.productId,
            firstPartyOrigin = servedPage?.origin,
        )

        val hostApiNavigation = NavigationPolicy.HostApiNavigation(
            onDeeplinkNavigation = { launchDeeplinkNavigation(it) },
            webViewLoader = { target -> scope.launch { webViewProvider.getWebView().loadUrl(target) } },
            dotNsTldProvider = dotNsTldProvider,
        )

        val cardFaceRequests = if (underCard) CardFaceRequests() else null
        val bridge = sessionStarter.start(
            webViewProvider,
            pageUrl,
            scope,
            hostApiNavigation,
            kind = if (underCard) ProductExecutionKind.WIDGET else ProductExecutionKind.APP,
            card = cardFaceRequests,
            explicitProductId = servedPage?.productId,
        )

        val currentUrlFlow = MutableStateFlow(pageUrl)
        webViewProvider.addOnPageStartedListener { url ->
            currentUrlFlow.value = url
            scope.launch {
                val productId = servedPage?.productId ?: dotNsTldProvider.getTld().getOrNull()
                    ?.let { tld -> ProductId.fromUrl(url.toUri(), tld).getOrNull() }
                productId?.let { productRegistrar.ensureRegistered(it) }
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

        return TrUAPISpaHostSession(
            webViewFlow,
            currentUrlFlow,
            loadProgressFlow,
            titleFlow,
            webViewProvider,
            bridge,
            cardFaceRequests?.requests ?: emptyFlow(),
        )
    }

    // A product with no published app is served from the developer's machine, so its page's host
    // says nothing about which product it is.
    private fun debugServedPage(launchUrl: String): DebugServedPage? {
        val tld = dotNsTldProvider.currentTldOrNull() ?: return null
        val productId = ProductId.fromUrl(launchUrl.toUri(), tld).getOrNull() ?: return null
        val appUrl = debugPocketCards.appUrl(productId) ?: return null
        val servedUrl = debugServedUrl(appUrl, launchUrl)
        val served = URI(servedUrl)

        return DebugServedPage(productId, servedUrl, origin = "${served.scheme}://${served.authority}")
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

private class DebugServedPage(val productId: ProductId, val url: String, val origin: String)

private class TrUAPISpaHostSession(
    override val webView: StateFlow<WebView?>,
    override val currentUrl: StateFlow<String>,
    override val loadProgress: StateFlow<DotNsLoadProgress>,
    override val title: StateFlow<String>,
    private val provider: BrowserWebViewProvider,
    private val bridge: ProductTrUAPIHostBridge,
    override val faceShownRequests: Flow<FaceShownRequest>,
) : SpaHostSession {
    override fun pauseConnections() {
        provider.pauseConnections()
    }

    override fun resumeConnections() {
        provider.resumeConnections()
    }
}
