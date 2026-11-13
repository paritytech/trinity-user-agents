package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_impl.data.config.FundingProvidersConfig
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import javax.inject.Inject

/**
 * Lists the funding providers the core offers: the configured ones after each Remote Config sync, which the core
 * ignores when nothing changed.
 */
class TrUAPIFundingProviders @Inject constructor(
    private val fundingProvidersConfig: FundingProvidersConfig,
) {
    /** Follows Remote Config in [scope] for as long as it lives. */
    fun install(runtime: TrUAPIHostRuntime, scope: CoroutineScope) {
        scope.launch {
            fundingProvidersConfig.publishedNames().collect { names -> offer(runtime, names) }
        }
    }

    private fun offer(runtime: TrUAPIHostRuntime, names: List<String>) {
        runCatching { runtime.setFundingProviderConfig(names) }
            .logFailure("TrUAPI refused the funding providers")
    }
}
