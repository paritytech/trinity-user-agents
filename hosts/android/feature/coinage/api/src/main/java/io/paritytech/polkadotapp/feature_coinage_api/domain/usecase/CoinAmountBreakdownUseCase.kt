package io.paritytech.polkadotapp.feature_coinage_api.domain.usecase

import io.paritytech.polkadotapp.feature_coinage_api.domain.common.CoinAmountBreakdown

interface CoinAmountBreakdownUseCase {
    suspend fun createCoinAmountBreakdown(): Result<CoinAmountBreakdown>
}
