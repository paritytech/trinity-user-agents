package io.paritytech.polkadotapp.feature_products_impl.domain.webView

import android.graphics.Bitmap
import android.net.Uri
import android.webkit.WebBackForwardList
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.core.net.toUri
import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.notFoundResponse
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.CallingProductIdProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.getProductIdOrNull
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.ProductPermissionGuard
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.handlers.NetworkAccessPermissionHandler
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withContext
import timber.log.Timber
import uniffi.truapi.RemotePermission
import uniffi.truapi.RemotePermissionRequest
import javax.inject.Inject

class WebViewPermissionClientFactory @Inject constructor(
    private val permissionGuard: ProductPermissionGuard,
    private val dotNsTldProvider: DotNsTldProvider
) {
    fun create(
        callingProductIdProvider: CallingProductIdProvider,
        firstPartyOrigin: String?,
    ): WebViewPermissionClient {
        return WebViewPermissionClient(callingProductIdProvider, firstPartyOrigin, permissionGuard, dotNsTldProvider)
    }
}

class WebViewPermissionClient(
    private val productIdProvider: CallingProductIdProvider,
    // Origin serving the calling product's own executable, when that origin is off-dotNS. A manifest
    // executable lives on `<kind>.<base>`, which the product id check below already covers; a
    // debug-menu worker is served from an arbitrary host that is nonetheless first-party.
    private val firstPartyOrigin: String?,
    private val permissionGuard: ProductPermissionGuard,
    private val dotNsTldProvider: DotNsTldProvider
) : WebViewClient() {
    @Volatile
    private var remoteAuthorization: (suspend (ProductId, String) -> Boolean)? = null

    fun useTrUAPIPermissions(productId: ProductId, execution: TrUAPIProductExecution, scope: CoroutineScope) {
        remoteAuthorization = { caller, domain ->
            caller == productId && withContext(scope.coroutineContext) {
                execution.authorizeRemotePermission(RemotePermissionRequest(RemotePermission.Remote(listOf(domain))))
            }
        }
    }

    /**
     * Last two entries from back-forward history, captured on [onPageStarted].
     *
     * Allows stale sub-resources (e.g. favicon) from the previous site during navigation, since
     * [shouldInterceptRequest] can fire for them before [onPageStarted] is invoked for the new page.
     */
    @Volatile
    private var recentProductIds: List<ProductId> = emptyList()

    override fun onPageStarted(view: WebView, url: String?, favicon: Bitmap?) {
        val history = view.copyBackForwardList()
        val current = history.currentIndex
        recentProductIds = listOfNotNull(
            history.productIdAt(current),
            history.productIdAt(current - 1),
        )
    }

    private fun WebBackForwardList.productIdAt(index: Int): ProductId? {
        if (index < 0 || index >= size) return null
        val url = getItemAtIndex(index)?.url ?: return null
        val tld = dotNsTldProvider.currentTldOrNull() ?: return null
        return ProductId.fromUrl(url.toUri(), tld).getOrNull()
    }

    override fun shouldInterceptRequest(view: WebView, request: WebResourceRequest?): WebResourceResponse? {
        val url = request?.url ?: return null
        if (url.scheme == "data") return null
        if (request.isForMainFrame) return null

        val callingProductId = runBlocking { productIdProvider.getProductIdOrNull() }

        if (callingProductId == null) {
            Timber.e("WebView Permissions: Blocked outbound request since calling product is null")
            return notFoundResponse()
        }

        val requestProductId = dotNsTldProvider.currentTldOrNull()?.let { tld ->
            ProductId.fromUrl(url, tld).getOrNull()
        }
        if (requestProductId == callingProductId || requestProductId in recentProductIds || url.isFirstParty()) {
            return null
        }

        val domain = NetworkAccessPermissionHandler.extractDomain(url.toString())
        if (domain != null) {
            val granted = runCatching {
                runBlocking {
                    val authorization = remoteAuthorization
                    if (authorization != null) {
                        authorization(callingProductId, domain)
                    } else {
                        permissionGuard.consumePermission(callingProductId, ProductPermission.RemotePermission.NetworkAccess(domain))
                    }
                }
            }.logFailure("WebView HTTP authorization failed").getOrDefault(false)
            if (granted) {
                return null
            }
        }

        Timber.w("Blocked outbound request from product $callingProductId: $url")
        return WebResourceResponse("text/plain", "UTF-8", 403, "Forbidden", emptyMap(), null)
    }

    private fun Uri.isFirstParty(): Boolean {
        val origin = originOrNull() ?: return false
        return origin.equals(firstPartyOrigin, ignoreCase = true)
    }

    private fun Uri.originOrNull(): String? {
        val scheme = scheme ?: return null
        val host = host ?: return null
        val port = port.takeIf { it != -1 }?.let { ":$it" }.orEmpty()
        return "$scheme://$host$port"
    }
}
