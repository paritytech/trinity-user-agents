package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.paritytech.polkadotapp.common.utils.Urls
import io.paritytech.polkadotapp.common.utils.coerceToUnit
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.flattenUnit
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.mapAsync
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_products_api.domain.FundingDomainProvider
import io.paritytech.polkadotapp.feature_products_api.model.Executables
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import javax.inject.Inject

interface FundingProductsWarmUp {
    suspend fun warmUp()
}

class RealFundingProductsWarmUp @Inject constructor(
    private val fundingDomainProvider: FundingDomainProvider,
    private val resolveProductUseCase: ResolveProductUseCase,
    private val dotNsResolver: DotNsResolver,
) : FundingProductsWarmUp {
    override suspend fun warmUp() {
        fundingDomainProvider.getFundingProductIds()
            .flatMap { productIds -> productIds.mapAsync { warmUp(it) }.flattenUnit() }
            .logFailure("Failed to warm up funding products")
    }

    private suspend fun warmUp(productId: ProductId): Result<Unit> {
        return resolveProductUseCase.resolve(productId)
            .flatMap { resolved -> resolved.executables.spaHosts() }
            .flatMap { hosts -> hosts.mapAsync { dotNsResolver.resolveToLocalUri(it).coerceToUnit() }.flattenUnit() }
    }

    // The SPA sheet loads the app archive and the worker script; the widget never shows there.
    private fun Executables.spaHosts(): Result<List<String>> = runCatching {
        listOfNotNull(app?.host?.value, worker?.let { Urls.hostOf(it.scriptUrl) })
    }
}
