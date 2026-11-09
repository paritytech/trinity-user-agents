package io.paritytech.polkadotapp.feature_products_impl.domain.browser

import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.bundled.BundledProducts
import io.paritytech.polkadotapp.feature_products_impl.domain.bundled.appHost
import uniffi.truapi.ProductId
import uniffi.truapi.ProductSource
import uniffi.truapi.productSource
import javax.inject.Inject

/** Decides once, when a page or Worker opens, where the product it names runs from; the core holds the rule. */
class ProductSources @Inject constructor(
    private val bundledProducts: BundledProducts,
    private val devServers: DevServers,
    private val dotNsTldProvider: DotNsTldProvider,
) {
    fun of(urlOrId: String): ProductSource? = productSource(
        urlOrId,
        dotNsTldProvider.currentTldOrNull()?.value.orEmpty(),
        devServers.enabled,
        bundledProducts.entries,
    )
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

/** Whether only the TrUAPI runtime serves it, whichever runtime the toggle selects. */
val ProductSource?.needsTrUAPI: Boolean
    get() = this is ProductSource.Bundled || this is ProductSource.DevServer
