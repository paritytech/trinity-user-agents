package io.paritytech.polkadotapp.feature_coinage_api.domain.usecase

import io.paritytech.polkadotapp.feature_coinage_api.domain.common.CoinageBalanceConversionContext

interface CoinageBalanceConverterUseCase {
    suspend fun create(): Result<CoinageBalanceConversionContext>
}
