package io.paritytech.polkadotapp.feature_coinage_impl.domain.common

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.chains.util.amountFromPlanks
import io.paritytech.polkadotapp.feature_coinage_api.domain.common.CoinageBalanceConversionContext
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.tokenAmount
import java.math.BigDecimal
import java.math.BigInteger
import java.math.RoundingMode

class RealCoinageBalanceConversionContext(
    private val unit: BigInteger,
    private val precision: Int
) : CoinageBalanceConversionContext {
    override fun formatExponentToBalance(exponent: ValueExponent): Balance {
        val amount = exponent.tokenAmount()

        return (amount * unit.toBigDecimal())
            .setScale(0, RoundingMode.DOWN)
            .toBigInteger()
            .intoBalance()
    }

    override fun formatExponentToAmount(exponent: ValueExponent): BigDecimal {
        return formatExponentToBalance(exponent).amountFromPlanks(precision)
    }
}
