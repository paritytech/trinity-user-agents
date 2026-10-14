package io.paritytech.polkadotapp.feature_coinage_impl.domain.usecase

import io.paritytech.polkadotapp.common.utils.combine
import io.paritytech.polkadotapp.feature_coinage_api.domain.common.CoinAmountBreakdown
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinAmountBreakdownUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageBalanceConverterUseCase
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.ExponentBoundsRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.getAllowedExponents
import io.paritytech.polkadotapp.feature_coinage_impl.domain.common.RealCoinAmountBreakdownContext
import io.paritytech.polkadotapp.feature_tokens_api.di.DigitalDollarChainAssetProvider
import io.paritytech.polkadotapp.feature_tokens_api.domain.ChainAssetProvider
import javax.inject.Inject

class RealCoinAmountBreakdownUseCase @Inject constructor(
    private val exponentBoundsRepository: ExponentBoundsRepository,
    private val coinageBalanceConvertionUseCase: CoinageBalanceConverterUseCase,
    @param:DigitalDollarChainAssetProvider private val chainAssetProvider: ChainAssetProvider
) : CoinAmountBreakdownUseCase {
    override suspend fun createCoinAmountBreakdown(): Result<CoinAmountBreakdown> {
        return combine(
            exponentBoundsRepository.getAllowedExponents(chainAssetProvider.chain().id),
            coinageBalanceConvertionUseCase.create()
        ).map { (exponents, convertionContext) ->
            RealCoinAmountBreakdownContext(
                precision = chainAssetProvider.asset().precision,
                coinageBalanceConvertionContext = convertionContext,
                allowedExponents = exponents
            )
        }
    }
}
