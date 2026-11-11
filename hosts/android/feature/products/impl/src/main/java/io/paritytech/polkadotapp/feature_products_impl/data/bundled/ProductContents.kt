package io.paritytech.polkadotapp.feature_products_impl.data.bundled

import io.paritytech.polkadotapp.feature_products_impl.domain.browser.ProductSources
import io.paritytech.polkadotapp.feature_products_impl.domain.bundled.BundledProducts
import uniffi.truapi.ProductId
import uniffi.truapi.ProductSource
import javax.inject.Inject

/**
 * What a WebView serves from the app itself for a product, by where it runs from: a bundled product's files, and
 * nothing for any other source, whose pages the WebView's dotNS or network clients load. The WebView providers ask
 * this, so none of them knows the bundle exists.
 */
class ProductContents @Inject constructor(
    private val bundledProducts: BundledProducts,
    private val productSources: ProductSources,
) {
    /** The pages a product opened from [source] is served from. */
    fun app(source: ProductSource?, mainDocumentResponseHeaders: Map<String, String>): BundledProductContent? =
        (source as? ProductSource.Bundled)?.let {
            BundledProductContent.app(it.entry, bundledProducts.files, mainDocumentResponseHeaders)
        }

    /** The worker module of [productId] is served from, decided as its pages are. */
    fun worker(productId: ProductId): BundledProductContent? =
        (productSources.of(productId.value) as? ProductSource.Bundled)?.let {
            BundledProductContent.worker(it.entry, bundledProducts.files)
        }
}
