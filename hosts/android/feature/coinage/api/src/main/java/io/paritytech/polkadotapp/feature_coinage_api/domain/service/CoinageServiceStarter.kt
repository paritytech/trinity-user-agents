package io.paritytech.polkadotapp.feature_coinage_api.domain.service
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope

interface CoinageServiceStarter {
    context(scope: ComputationalScope)
    fun start()
}
