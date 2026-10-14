package io.paritytech.polkadotapp.feature_products_impl.domain.product

import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_products_api.domain.product.ProductContentWarmUp
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.ProductRuntimeSettings
import io.paritytech.polkadotapp.feature_products_api.model.ExecutableKind
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.ProductServingHostResolver
import javax.inject.Inject

/**
 * Asks for exactly what the WebView under the card will ask for when it loads, so both land on the same
 * cache entry: the resolver holds its lock across a fetch, and the load joins it instead of
 * starting a second one.
 */
class RealProductContentWarmUp @Inject constructor(
    private val dotNsResolver: DotNsResolver,
    private val servingHostResolver: ProductServingHostResolver,
    private val runtimeSettings: ProductRuntimeSettings,
) : ProductContentWarmUp {
    override suspend fun warmUp(productId: ProductId): Result<Unit> =
        runCancellableCatching { servingHostResolver.servingHostFor(productId.value, cardExecutable()) }
            .flatMap { servingHost -> dotNsResolver.resolveToLocalUri(servingHost) }
            .map {}

    // Only the TrUAPI runtime serves a card its widget; the native runtime still serves the app under it.
    private fun cardExecutable(): ExecutableKind =
        if (runtimeSettings.isTrUAPIRuntimeEnabled()) ExecutableKind.WIDGET else ExecutableKind.APP
}
