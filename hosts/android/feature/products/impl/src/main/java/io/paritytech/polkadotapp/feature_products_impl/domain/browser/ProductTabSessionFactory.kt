package io.paritytech.polkadotapp.feature_products_impl.domain.browser

import android.net.Uri
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.ProductRuntimeSettings
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.BrowserWebViewProvider
import kotlinx.coroutines.CoroutineScope
import javax.inject.Inject

/**
 * Builds the live runtime for a single browser tab on the host runtime
 * [ProductRuntimeSettings] selects for where the product runs from, read per
 * tab at [create]. Where the product runs from is decided here, once, and
 * passed down.
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

        return if (runtimeSettings.isTrUAPIRuntimeEnabledFor(source)) {
            truapi.create(url, scope, onDeeplink, source)
        } else {
            native.create(url, scope, onDeeplink)
        }
    }
}
