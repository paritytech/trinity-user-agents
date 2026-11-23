package io.paritytech.polkadotapp.feature_coinage_impl.domain.recycling

import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.CoinageRecyclingStrategySettings
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.RecyclingParams
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.params
import io.paritytech.polkadotapp.feature_coinage_impl.data.config.CoinagePrivacyConfigProvider
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.combine
import javax.inject.Inject

/**
 * The preset the user chose, resolved against the activated privacy config.
 *
 * Both halves are needed to answer any question about recycling, and combining them here is what keeps a
 * deadline timer and the usability check it feeds from ever reading different values.
 */
class RecyclingParamsProvider @Inject constructor(
    private val settings: CoinageRecyclingStrategySettings,
    private val privacyConfigProvider: CoinagePrivacyConfigProvider,
) {
    fun paramsUpdates(): Flow<RecyclingParams> = combine(
        settings.strategyFlow(),
        privacyConfigProvider.privacyConfigUpdates(),
    ) { type, privacyConfig -> type.params(privacyConfig) }

    suspend fun currentParams(): RecyclingParams =
        settings.getStrategy().params(privacyConfigProvider.privacyConfigOrDefault())
}
