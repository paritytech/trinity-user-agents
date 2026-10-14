package io.paritytech.polkadotapp.feature_fund_impl.data.model

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapFee
import io.paritytech.polkadotapp.feature_swap_api.domain.model.SwapQuote

internal class ConversionTerms(
    val fee: SwapFee,
    val minDepositAmount: Balance,
    val midSizeDepositQuote: SwapQuote,
)
