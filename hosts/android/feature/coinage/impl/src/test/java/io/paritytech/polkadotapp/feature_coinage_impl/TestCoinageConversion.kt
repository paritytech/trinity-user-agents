package io.paritytech.polkadotapp.feature_coinage_impl

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.feature_coinage_api.domain.common.CoinageBalanceConversionContext
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import java.math.BigDecimal
import java.math.BigInteger

/** A denomination is worth 2^exponent planks, so amounts in tests read as sums of powers of two. */
object PowerOfTwoConversion : CoinageBalanceConversionContext {
    override fun formatExponentToBalance(exponent: ValueExponent): Balance =
        BigInteger.TWO.pow(exponent.value).intoBalance()

    override fun formatExponentToAmount(exponent: ValueExponent): BigDecimal =
        BigDecimal(formatExponentToBalance(exponent).value)
}

fun planks(value: Int): Balance = value.toBigInteger().intoBalance()
