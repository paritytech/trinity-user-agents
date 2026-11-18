package io.paritytech.polkadotapp.feature_coinage_impl.core

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_coinage_api.domain.recycling.CoinageRecyclingStrategySettings
import io.paritytech.polkadotapp.feature_coinage_api.domain.service.CoinageServiceStarter
import io.paritytech.polkadotapp.feature_usernames_api.domain.usecase.ObserveAccountOnboardingStatusUseCase
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import javax.inject.Inject

/** Starts the core's coinage layer once the account exists, and keeps its privacy setting the user's choice. */
class CoreCoinageServiceStarter @Inject constructor(
    private val core: CoinageCore,
    private val strategySettings: CoinageRecyclingStrategySettings,
    private val observeAccountOnboardingStatusUseCase: ObserveAccountOnboardingStatusUseCase,
) : CoinageServiceStarter {
    context(scope: ComputationalScope)
    override fun start() {
        scope.launch {
            observeAccountOnboardingStatusUseCase().filter { it.isOnboarded }.first()
            strategySettings.strategyFlow().distinctUntilChanged().collect { strategy ->
                core.use { it.setPrivacy(strategy.toPreset()) }.logFailure("Failed to apply the coinage privacy setting")
            }
        }
    }
}
