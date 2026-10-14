package io.paritytech.polkadotapp.feature_coinage_api.domain.usecase

import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageBalance
import kotlinx.coroutines.flow.Flow

interface TotalBalanceUseCase {
    fun subscribeTotalBalance(): Flow<Result<CoinageBalance>>

    suspend fun getBalance(): Result<CoinageBalance>
}
