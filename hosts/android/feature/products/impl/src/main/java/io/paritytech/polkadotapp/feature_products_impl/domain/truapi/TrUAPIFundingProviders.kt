package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.feature_products_impl.data.config.FundingProvidersConfig
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import javax.inject.Inject

// No dedup: the core ignores an unchanged list.
class TrUAPIFundingProviders @Inject constructor(
    private val fundingProvidersConfig: FundingProvidersConfig,
) {
    fun install(runtime: TrUAPIHostRuntime, scope: CoroutineScope) {
        scope.launch {
            fundingProvidersConfig.publishedNames().collect { names -> runtime.setFundingProviderConfig(names) }
        }
    }
}
