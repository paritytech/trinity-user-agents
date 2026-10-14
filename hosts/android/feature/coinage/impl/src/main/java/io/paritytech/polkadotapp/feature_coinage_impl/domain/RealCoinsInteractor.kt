package io.paritytech.polkadotapp.feature_coinage_impl.domain

import io.paritytech.polkadotapp.feature_coinage_api.domain.CoinsInteractor
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.CoinRepository
import kotlinx.coroutines.flow.Flow
import javax.inject.Inject

class RealCoinsInteractor @Inject constructor(
    private val coinRepository: CoinRepository
) : CoinsInteractor {
    override fun subscribeCoins(): Flow<List<Coin>> {
        return coinRepository.subscribeAllCoins()
    }
}
