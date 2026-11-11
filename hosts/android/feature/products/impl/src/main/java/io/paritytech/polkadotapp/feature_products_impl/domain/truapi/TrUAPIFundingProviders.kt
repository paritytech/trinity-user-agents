package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_impl.data.config.FundingProvidersConfig
import io.paritytech.polkadotapp.feature_products_impl.domain.bundled.BundledProducts
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import uniffi.truapi.FundingProviderEntry
import javax.inject.Inject

/**
 * Lists the funding providers the core offers: the bundled ones at once, then the configured ones after each Remote
 * Config sync, which the core merges and ignores when nothing changed.
 */
class TrUAPIFundingProviders @Inject constructor(
    private val bundledProducts: BundledProducts,
    private val fundingProvidersConfig: FundingProvidersConfig,
) {
    /** Follows Remote Config in [scope] for as long as it lives. */
    fun install(runtime: TrUAPIHostRuntime, scope: CoroutineScope) {
        val bundled = bundledProducts.fundingProviders()
        if (bundled.isNotEmpty()) offer(runtime, emptyList(), bundled)
        scope.launch {
            fundingProvidersConfig.publishedNames().collect { names -> offer(runtime, names, bundled) }
        }
    }

    private fun offer(runtime: TrUAPIHostRuntime, names: List<String>, bundled: List<FundingProviderEntry>) {
        runCatching { runtime.setFundingProviderConfig(names, bundled) }
            .logFailure("TrUAPI refused the funding providers")
    }
}
