package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.net.Uri
import androidx.core.net.toUri
import io.paritytech.polkadotapp.common.data.telemetry.HostLoadContext
import io.paritytech.polkadotapp.common.data.telemetry.HostLoadTrigger
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.model.productIdFromUrl
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.navigation.NavigationPolicy
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.BrowserWebViewProvider
import io.paritytech.polkadotapp.feature_products_impl.presentation.spaHost.ExpandedCardFace
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import timber.log.Timber
import uniffi.truapi.ExecutableKind
import uniffi.truapi.ProductId
import java.util.concurrent.atomic.AtomicBoolean
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
    private val visibleProducts: VisibleProducts,
    private val hostWebViewCreatedTelemetry: HostWebViewCreatedTelemetry,
    private val hostPageOpenTelemetry: HostPageOpenTelemetry,
) {
    fun start(
        provider: BrowserWebViewProvider,
        productUrl: String,
        scope: CoroutineScope,
        hostApiNavigation: NavigationPolicy,
        card: ExpandedCardFace? = null,
        explicitProductId: ProductId? = null,
    ): ProductTrUAPIHostBridge = startPage(
        provider,
        productUrl,
        scope,
        hostApiNavigation,
        card,
        explicitProductId,
        hostPageOpenTelemetry.open(),
    )

    /** Starts the page as [start] does, reporting its open through [pageOpen], which the caller keeps. */
    fun startPage(
        provider: BrowserWebViewProvider,
        productUrl: String,
        scope: CoroutineScope,
        hostApiNavigation: NavigationPolicy,
        card: ExpandedCardFace?,
        explicitProductId: ProductId?,
        pageOpen: HostPageOpenTelemetry.PageOpen,
    ): ProductTrUAPIHostBridge {
        scope.coroutineContext[Job]?.invokeOnCompletion { pageOpen.closed() }
        val bridge = hostBridgeFactory.create(scope)
        scope.launch {
            attachAndLoad(bridge, provider, productUrl, hostApiNavigation, card, explicitProductId, pageOpen)
                // A page closed mid-start is abandoned, which closing it reports.
                .onFailure { if (isActive) pageOpen.failedToStart() }
                .logFailure("Failed to start TrUAPI host bridge for $productUrl")
        }
        return bridge
    }

    private suspend fun attachAndLoad(
        bridge: ProductTrUAPIHostBridge,
        provider: BrowserWebViewProvider,
        productUrl: String,
        navigation: NavigationPolicy,
        card: ExpandedCardFace?,
        explicitProductId: ProductId?,
        pageOpen: HostPageOpenTelemetry.PageOpen,
    ): Result<Unit> {
        // A debug page served from a loopback url carries no product in its host.
        val productId = explicitProductId ?: run {
            val tld = dotNsTldProvider.getTld().getOrElse { return Result.failure(it) }
            // A page that is not a product (the debug SPA browser opening any URL) has no bridge to
            // attach, but it is still a page to show.
            productIdFromUrl(productUrl.toUri(), tld).getOrNull() ?: run {
                Timber.d("Loading %s without a TrUAPI bridge: not a product URL", productUrl)
                pageOpen.notAProduct()
                return runCatching { provider.loadInitialContent() }
            }
        }

        val hostSessionId = openSession(productId, provider, pageOpen)

        provider.addWebViewSetup { webView -> visibleProducts.track(productId, webView) }
        val runtime = runtimeProvider.runtime().getOrElse { return Result.failure(it) }
        // The bootstrap publishes the loopback port and its bearer token, so it goes to the
        // product's own origin only. A wildcard would hand the bridge endpoint to any page the
        // WebView is ever pointed at.
        val installBootstrap = runCatching {
            bootstrapInstaller.installerFor(
                origins = setOf(productUrl.toUri().origin()),
                hostSessionId = hostSessionId,
                pageOrigin = pageOpen.pageOrigin,
            )
        }.getOrElse { return Result.failure(it) }

        val kind = if (card != null) ExecutableKind.WIDGET else ExecutableKind.APP
        val webViewEventReported = AtomicBoolean(false)
        return bridge
            .attach(
                runtime,
                productId,
                chainDirectory.resolve(),
                navigation,
                kind,
                card = card,
                hostSessionId = hostSessionId,
            ) { bootstrap ->
                val setup = installBootstrap(bootstrap)
                provider.addWebViewSetup { webView ->
                    setup(webView)
                    if (webViewEventReported.compareAndSet(false, true)) {
                        hostWebViewCreatedTelemetry.report(hostSessionId)
                    }
                }
            }
            .mapCatching { execution ->
                provider.useTrUAPIPermissions(productId, execution)
                provider.loadInitialContent()
            }
    }

    /**
     * The page's host session: the one its tap or link minted, so the page's lookups join the fetch
     * started at the tap, or a new one. It travels with this page only: the WebView's archive lookups
     * carry it, so a second page of the same product, or a prewarm, never takes or drops it.
     */
    private fun openSession(
        productId: ProductId,
        provider: BrowserWebViewProvider,
        pageOpen: HostPageOpenTelemetry.PageOpen,
    ): String {
        val hostSessionId = pageOpen.opened(productId)
        provider.useHostLoad(HostLoadContext(HostLoadTrigger.OPEN, hostSessionId))

        val mainFrameFailed = AtomicBoolean(false)
        provider.addOnPageStartedListener { mainFrameFailed.set(false) }
        provider.addOnMainFrameFailedListener { mainFrameFailed.set(true) }
        provider.addOnPageFinishedListener { pageOpen.finished(loaded = !mainFrameFailed.get()) }
        provider.addOnRevealedListener(pageOpen::revealed)
        return hostSessionId
    }

    private fun Uri.origin(): String = buildString {
        append(scheme).append("://").append(host)
        port.takeIf { it != -1 }?.let { append(':').append(it) }
    }
}
