package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.net.Uri
import androidx.core.net.toUri
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.navigation.NavigationPolicy
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.BrowserWebViewProvider
import io.paritytech.polkadotapp.feature_products_impl.presentation.spaHost.ExpandedCardFace
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import timber.log.Timber
import uniffi.truapi.ProductExecutionKind
import javax.inject.Inject

/**
 * Boots a [ProductTrUAPIHostBridge] for a product WebView: opens its execution,
 * registers the bootstrap at document start, and only then triggers
 * the initial page load — the bootstrap must be in place before the page loads
 * or the product never connects.
 */
class TrUAPISessionStarter @Inject constructor(
    private val hostBridgeFactory: ProductTrUAPIHostBridge.Factory,
    private val runtimeProvider: TrUAPIHostRuntimeProvider,
    private val chainDirectory: TrUAPIChainDirectory,
    private val dotNsTldProvider: DotNsTldProvider,
    private val bootstrapInstaller: TrUAPIBootstrapInstaller,
) {
    fun start(
        provider: BrowserWebViewProvider,
        productUrl: String,
        scope: CoroutineScope,
        hostApiNavigation: NavigationPolicy,
        kind: ProductExecutionKind = ProductExecutionKind.APP,
        card: ExpandedCardFace? = null,
    ): ProductTrUAPIHostBridge {
        val bridge = hostBridgeFactory.create(scope)
        scope.launch {
            attachAndLoad(bridge, provider, productUrl, hostApiNavigation, kind, card)
                .logFailure("Failed to start TrUAPI host bridge for $productUrl")
        }
        return bridge
    }

    private suspend fun attachAndLoad(
        bridge: ProductTrUAPIHostBridge,
        provider: BrowserWebViewProvider,
        productUrl: String,
        navigation: NavigationPolicy,
        kind: ProductExecutionKind,
        card: ExpandedCardFace?,
    ): Result<Unit> {
        val tld = dotNsTldProvider.getTld().getOrElse { return Result.failure(it) }
        // A page that is not a product (the debug SPA browser opening any URL) has no bridge to
        // attach, but it is still a page to show.
        val productId = ProductId.fromUrl(productUrl.toUri(), tld).getOrNull() ?: run {
            Timber.d("Loading %s without a TrUAPI bridge: not a product URL", productUrl)
            return runCatching { provider.loadInitialContent() }
        }

        val runtime = runtimeProvider.runtime().getOrElse { return Result.failure(it) }
        // The bootstrap publishes the loopback port and its bearer token, so it goes to the
        // product's own origin only. A wildcard would hand the bridge endpoint to any page the
        // WebView is ever pointed at.
        val installBootstrap = runCatching {
            bootstrapInstaller.installerFor(setOf(productUrl.toUri().origin()))
        }.getOrElse { return Result.failure(it) }

        return bridge
            .attach(runtime, productId, chainDirectory.resolve(), navigation, kind, card) { bootstrap ->
                provider.addWebViewSetup(installBootstrap(bootstrap))
            }
            .mapCatching { execution ->
                provider.useTrUAPIPermissions(productId, execution)
                provider.loadInitialContent()
            }
    }

    private fun Uri.origin(): String = buildString {
        append(scheme).append("://").append(host)
        port.takeIf { it != -1 }?.let { append(':').append(it) }
    }
}
