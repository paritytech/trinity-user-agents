package io.paritytech.polkadotapp.feature_products_impl.domain.product.update

import io.paritytech.polkadotapp.common.utils.coerceToUnit
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.flattenResult
import io.paritytech.polkadotapp.common.utils.mapAsync
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsUpdates
import io.paritytech.polkadotapp.feature_products_api.model.productIdFromHost
import javax.inject.Inject

class ProductUpdater @Inject constructor(
    private val dotNsUpdates: DotNsUpdates,
    private val dotNsTldProvider: DotNsTldProvider,
) {
    suspend fun checkAll(): Result<Unit> = dotNsTldProvider.getTld().flatMap { tld ->
        dotNsUpdates.namesOnDevice()
            .groupBy { dotNsName -> productIdFromHost(dotNsName, tld).getOrNull()?.value ?: dotNsName }
            .values
            .mapAsync { productNames -> dotNsUpdates.fetchUpdates(productNames) }
            .flattenResult()
            .coerceToUnit()
    }
}
