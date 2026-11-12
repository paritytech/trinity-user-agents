package io.paritytech.polkadotapp.feature_coinage_api.domain.common

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import java.math.BigDecimal

interface CoinAmountBreakdown {
    fun breakdown(amount: BigDecimal): List<ValueExponent>

    fun roundDownAmount(amount: BigDecimal): BigDecimal

    /** The part of [amount], in planks, that is not a multiple of the smallest coin. */
    fun remainderAfterBreakdown(amount: BigDecimal): Balance
}

fun CoinAmountBreakdown.breakdownRoundDown(amount: BigDecimal): List<ValueExponent> {
    return breakdown(roundDownAmount(amount))
}
