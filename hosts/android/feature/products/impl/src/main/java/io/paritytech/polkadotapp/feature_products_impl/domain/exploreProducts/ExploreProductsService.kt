package io.paritytech.polkadotapp.feature_products_impl.domain.exploreProducts

import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import javax.inject.Inject

interface ExploreProductsService {
    suspend fun getExploreUrl(): Result<String>

    suspend fun warmUpExploreLoading()
}

class RealExploreProductsService @Inject constructor(
    private val dotNsResolver: DotNsResolver,
    private val dotNsTldProvider: DotNsTldProvider,
) : ExploreProductsService {
    companion object {
        private const val BROWSE_LABEL = "browse"
    }

    override suspend fun getExploreUrl(): Result<String> {
        return dotNsTldProvider.getTld().map { tld -> "https://$BROWSE_LABEL${tld.suffix}" }
    }

    override suspend fun warmUpExploreLoading() {
        dotNsTldProvider.getTld()
            .flatMap { tld -> dotNsResolver.resolveToLocalUri("$BROWSE_LABEL${tld.suffix}") }
            .logFailure("Failed to warm up explore loading")
    }
}
