package io.paritytech.polkadotapp.feature_products_impl.domain.browser

import android.net.Uri
import androidx.core.net.toUri
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductsBotApiImpl
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.HostApiEnvironment
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.HostApiSession
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.HostCallGroupFactory
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.PageLoadInjection
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.navigation.NavigationPolicy
import io.paritytech.polkadotapp.feature_products_impl.domain.jsRuntime.WebViewRuntime
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.BrowserWebViewProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorkerRefCounter
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.withWorkerAcquired
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.launch
import timber.log.Timber
import javax.inject.Inject

/**
 * Builds the live runtime for a single browser tab on the native JS-bridge
 * host — the WebView provider plus its host-api session — all hosted in the
 * tab's [CoroutineScope] (cancelling it disposes the session and destroys the
 * WebView).
 */
class NativeProductTabSessionFactory @Inject constructor(
    private val browserWebViewProviderFactory: BrowserWebViewProvider.Factory,
    private val hostCallGroupFactory: HostCallGroupFactory,
    private val sessionFactory: HostApiSession.Factory,
    private val botApiFactory: ProductsBotApiImpl.Factory,
    private val dotNsTldProvider: DotNsTldProvider,
    private val workerRefCounter: ProductWorkerRefCounter,
) {
    fun create(url: String, scope: CoroutineScope, onDeeplink: (Uri) -> Unit): BrowserWebViewProvider {
        val provider = browserWebViewProviderFactory.create(
            url,
            NavigationPolicy.InlineNavigation(onDeeplinkNavigation = onDeeplink),
            allowIframes = true,
            scope,
        )

        // Keep the product's worker alive for as long as its full-page screen is open. A product
        // that publishes no worker is a no-op; the reference releases when the tab scope ends.
        scope.launch {
            val tld = dotNsTldProvider.getTld().getOrNull() ?: return@launch
            val productId = ProductId.fromUrl(url.toUri(), tld).getOrNull() ?: return@launch
            workerRefCounter.withWorkerAcquired(productId, "fullpage:${productId.value}") {
                awaitCancellation()
            }
        }

        val hostApiNavigation = NavigationPolicy.HostApiNavigation(
            onDeeplinkNavigation = onDeeplink,
            webViewLoader = { target -> scope.launch { provider.getWebView().loadUrl(target) } },
            dotNsTldProvider = dotNsTldProvider,
        )

        val callingProductIdProvider = provider.callingProductIdProvider
        val botApi = botApiFactory.create(callingProductIdProvider)
        val runtime = WebViewRuntime(provider)
        val transport = runtime.createTransport()
        val handlerGroups = hostCallGroupFactory.createShared(botApi, callingProductIdProvider, hostApiNavigation)
        val environment = HostApiEnvironment(
            injectionStrategy = PageLoadInjection(pageLifecycleSource = provider, coroutineScope = scope),
            handlerGroups = handlerGroups,
        )
        val session = sessionFactory.create(environment, runtime, transport, scope)

        scope.launch {
            runCatching { session.initialize() }
                .onFailure { Timber.e(it, "Failed to initialize browser tab") }
        }

        return provider
    }
}
