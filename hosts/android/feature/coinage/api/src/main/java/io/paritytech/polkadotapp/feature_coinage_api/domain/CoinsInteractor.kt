package io.paritytech.polkadotapp.feature_coinage_api.domain

import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import kotlinx.coroutines.flow.Flow

interface CoinsInteractor {
    fun subscribeCoins(): Flow<List<Coin>>
}
