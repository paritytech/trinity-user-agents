package io.paritytech.polkadotapp.feature_wallet_impl.domain.funding

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageBalanceConverterUseCase
import io.paritytech.polkadotapp.feature_coinage_api.domain.usecase.CoinageHoldingsUseCase
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingBalance
import io.paritytech.polkadotapp.feature_wallet_impl.domain.model.toHoldingsInfo
import kotlinx.coroutines.flow.first
import javax.inject.Inject

/** The card's Ready figure, which is what a withdrawal can spend. */
internal class CoinageFundingBalance @Inject constructor(
    private val coinageHoldingsUseCase: CoinageHoldingsUseCase,
    private val coinageBalanceConverterUseCase: CoinageBalanceConverterUseCase,
) : FundingBalance {
    override suspend fun spendable(): Balance? {
        val holdings = coinageHoldingsUseCase.subscribeHoldings().first()

        return coinageBalanceConverterUseCase.create()
            .map { conversion -> with(conversion) { holdings.toHoldingsInfo() }.balance.availablePrivate }
            .logFailure("CoinageFundingBalance: Failed to classify coinage holdings")
            .getOrNull()
    }
}
