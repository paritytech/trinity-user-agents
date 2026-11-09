package io.paritytech.polkadotapp.feature_products_impl.domain.browser

import android.net.Uri
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.ProductRuntimeSettings
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.BrowserWebViewProvider
import kotlinx.coroutines.CoroutineScope
import javax.inject.Inject

/**
 * Builds the live runtime for a single browser tab on the host runtime
 * selected via [ProductRuntimeSettings], read per tab at [create].
 * A development server always runs on TrUAPI: the core is what gives it
 * its product id, whichever runtime the toggle selects. So does a product
 * the app ships, which only TrUAPI serves. Where the product runs from is
 * decided here, once, and passed down.
 * [RealProductSessionController] just attaches its listeners to the returned
 * provider.
 */
class ProductTabSessionFactory @Inject constructor(
    private val native: NativeProductTabSessionFactory,
    private val truapi: TrUAPIProductTabSessionFactory,
    private val runtimeSettings: ProductRuntimeSettings,
    private val productSources: ProductSources,
) {
    fun create(url: String, scope: CoroutineScope, onDeeplink: (Uri) -> Unit): BrowserWebViewProvider {
        val source = productSources.of(url)

        return if (source.needsTrUAPI || runtimeSettings.isTrUAPIRuntimeEnabled()) {
            truapi.create(url, scope, onDeeplink, source)
        } else {
            native.create(url, scope, onDeeplink)
        }
    }
}
