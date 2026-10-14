package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi

import androidx.core.net.toUri
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.model.ProductId

/**
 * Provides the [ProductId] of the product that is currently calling the host API.
 * Different environments resolve this differently.
 */
fun interface CallingProductIdProvider {
    suspend fun getProductId(): Result<ProductId>
}

suspend fun CallingProductIdProvider.getProductIdOrNull(): ProductId? {
    return getProductId().getOrNull()
}

/**
 * Chat: product ID is fixed for the lifetime of the extension.
 */
class FixedProductId(private val productId: ProductId) : CallingProductIdProvider {
    override suspend fun getProductId(): Result<ProductId> = Result.success(productId)
}

/**
 * SPA/Explore: product ID is derived from the current WebView URL.
 */
class UrlDerivedProductId(
    private val dotNsTldProvider: DotNsTldProvider,
    private val urlProvider: suspend () -> String?
) : CallingProductIdProvider {
    override suspend fun getProductId(): Result<ProductId> {
        val url = urlProvider()
            ?: return Result.failure(IllegalStateException("No current URL available"))
        return dotNsTldProvider.getTld().flatMap { tld -> ProductId.fromUrl(url.toUri(), tld) }
    }
}
