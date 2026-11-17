package io.paritytech.polkadotapp.feature_products_impl.domain.browser

import androidx.core.net.toUri
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.model.productIdFromUrl
import io.paritytech.polkadotapp.feature_products_impl.domain.bundled.BundledProducts
import io.paritytech.polkadotapp.feature_products_impl.domain.bundled.appHost
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.DebugPocketCards
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.debugServedUrl
import uniffi.truapi.DevServerProduct
import uniffi.truapi.ProductId
import uniffi.truapi.ProductSource
import uniffi.truapi.productSource
import java.net.URI
import javax.inject.Inject

/** Decides once, when a page or Worker opens, where the product it names runs from; the core holds the rule. */
class ProductSources @Inject constructor(
    private val bundledProducts: BundledProducts,
    private val devServers: DevServers,
    private val dotNsTldProvider: DotNsTldProvider,
    private val debugPocketCards: DebugPocketCards,
) {
    fun of(urlOrId: String): ProductSource? = productSource(
        urlOrId,
        dotNsTldProvider.currentTldOrNull()?.value.orEmpty(),
        devServers.enabled,
        bundledProducts.entries,
    )

    /**
     * As [of], for a page opened in a SPA session: a published product whose page the debug menu serves from the
     * developer's machine runs from there.
     */
    fun ofSpaPage(launchUrl: String): ProductSource? = when (val source = of(launchUrl)) {
        is ProductSource.Bundled, is ProductSource.DevServer -> source
        is ProductSource.Published, null -> debugServedPage(launchUrl) ?: source
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

/** The id a page runs under whatever its URL says; `null` for a published product, read off each URL it loads. */
val ProductSource.fixedProductId: ProductId?
    get() = when (this) {
        is ProductSource.Bundled -> entry.productId
        is ProductSource.DevServer -> product.productId
        is ProductSource.Published -> null
    }

/** The origin a page loads from when it is not a dotNS host. */
val ProductSource.firstPartyOrigin: String?
    get() = when (this) {
        is ProductSource.Bundled -> "https://${entry.appHost}"
        is ProductSource.DevServer -> product.origin
        is ProductSource.Published -> null
    }
