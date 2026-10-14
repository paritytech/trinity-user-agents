package io.paritytech.polkadotapp.feature_coinage_impl.domain.planner

import io.paritytech.polkadotapp.common.utils.combineResults
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinAmountBreakdownUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageBalanceConverterUseCase
import io.paritytech.polkadotapp.feature_coinage_impl.data.repository.CoinRepository
import javax.inject.Inject

class TransferPlannerFactory @Inject constructor(
    private val breakdownAmountUseCase: CoinAmountBreakdownUseCase,
    private val coinageBalanceConvertionUseCase: CoinageBalanceConverterUseCase,
    private val coinRepository: CoinRepository
) {
    suspend fun create(): Result<TransferPlanner> {
        return combineResults(
            coinRepository.getCoinRecyclingAge(),
            breakdownAmountUseCase.createCoinAmountBreakdown(),
            coinageBalanceConvertionUseCase.create()
        ) { recyclingAge, breakdownAmount, convertionContext ->
            TransferPlanner(convertionContext, breakdownAmount, recyclingAge)
        }
    }
}
