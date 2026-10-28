package io.paritytech.polkadotapp.feature_coinage_api.domain.deposit

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope

interface AutoConvertDepositService {
    /**
     * Should be called once per session
     */
    context(scope: ComputationalScope)
    suspend fun startObserveAndConvert()
}
