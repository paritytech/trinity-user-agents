package io.paritytech.polkadotapp.feature_coinage_impl.domain.usecase

import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_coinage_api.domain.common.CoinageBalanceConversionContext
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageBalanceConverterUseCase
import io.paritytech.polkadotapp.feature_coinage_impl.data.config.CoinageInstanceIdProvider
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.CoinageInstanceRepository
import io.paritytech.polkadotapp.feature_coinage_impl.domain.common.RealCoinageBalanceConversionContext
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import javax.inject.Inject

class RealCoinageBalanceConverterUseCase @Inject constructor(
    private val coinageInstanceRepository: CoinageInstanceRepository,
    private val coinageInstanceIdProvider: CoinageInstanceIdProvider,
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider
) : CoinageBalanceConverterUseCase {
    override suspend fun create(): Result<CoinageBalanceConversionContext> {
        return coinageInstanceIdProvider.instanceId()
            .flatMap { instanceId -> coinageInstanceRepository.assetUnit(instanceId) }
            .mapCatching { assetUnit ->
                RealCoinageBalanceConversionContext(
                    unit = assetUnit,
                    precision = chainAssetProvider.asset().precision
                )
            }
    }
}
