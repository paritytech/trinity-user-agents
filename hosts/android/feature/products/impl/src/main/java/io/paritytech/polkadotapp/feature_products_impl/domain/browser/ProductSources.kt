package io.paritytech.polkadotapp.feature_products_impl.domain.browser

import androidx.core.net.toUri
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.model.productIdFromUrl
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.DebugPocketCards
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.debugServedUrl
import uniffi.truapi.DevServerProduct
import uniffi.truapi.ProductId
import uniffi.truapi.ProductSource
import uniffi.truapi.productSource
import java.net.URI
import javax.inject.Inject

// Decided once, when a page opens, and passed down: nothing below it reads the source off a URL again.
class ProductSources @Inject constructor(
    private val devServers: DevServers,
    private val dotNsTldProvider: DotNsTldProvider,
    private val debugPocketCards: DebugPocketCards,
) {
    fun of(url: String): ProductSource? = productSource(
        url,
        dotNsTldProvider.currentTldOrNull()?.value.orEmpty(),
        devServers.enabled,
    )

    fun ofSpaPage(launchUrl: String): ProductSource? {
        val source = of(launchUrl)
        if (source is ProductSource.DevServer) return source

        return debugServedPage(launchUrl) ?: source
    }

    // A product with no published app is served from the developer's machine, so its page's host
    // says nothing about which product it is.
    private fun debugServedPage(launchUrl: String): ProductSource.DevServer? {
        val tld = dotNsTldProvider.currentTldOrNull() ?: return null
        val productId = productIdFromUrl(launchUrl.toUri(), tld).getOrNull() ?: return null
        val appUrl = debugPocketCards.appUrl(productId) ?: return null
        val servedUrl = debugServedUrl(appUrl, launchUrl)
        val served = URI(servedUrl)

        return ProductSource.DevServer(DevServerProduct(productId, origin = "${served.scheme}://${served.authority}", url = servedUrl))
    }
}

// A dev server page loads from an address that reaches the developer's machine but runs under the id the core
// gave it, so neither can be read off the other. A published page reads its id off each URL it loads.
val ProductSource.fixedProductId: ProductId?
    get() = (this as? ProductSource.DevServer)?.product?.productId

val ProductSource.firstPartyOrigin: String?
    get() = (this as? ProductSource.DevServer)?.product?.origin
