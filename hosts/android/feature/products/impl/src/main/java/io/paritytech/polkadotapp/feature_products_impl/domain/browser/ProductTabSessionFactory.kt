package io.paritytech.polkadotapp.feature_products_impl.domain.browser

import android.net.Uri
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.ProductRuntimeSettings
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.BrowserWebViewProvider
import kotlinx.coroutines.CoroutineScope
import javax.inject.Inject

/**
 * Builds the live runtime for a single browser tab on the host runtime
 * selected via [ProductRuntimeSettings], read per tab at [create].
 * [RealProductSessionController] just attaches its listeners to the returned
 * provider.
 */
class ProductTabSessionFactory @Inject constructor(
    private val native: NativeProductTabSessionFactory,
    private val truapi: TrUAPIProductTabSessionFactory,
    private val runtimeSettings: ProductRuntimeSettings,
) {
    fun create(url: String, scope: CoroutineScope, onDeeplink: (Uri) -> Unit): BrowserWebViewProvider =
        if (runtimeSettings.isTrUAPIRuntimeEnabled()) {
            truapi.create(url, scope, onDeeplink)
        } else {
            native.create(url, scope, onDeeplink)
        }
}
