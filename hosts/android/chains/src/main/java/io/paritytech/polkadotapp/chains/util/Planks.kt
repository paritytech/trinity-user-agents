package io.paritytech.polkadotapp.chains.util

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import java.math.BigDecimal

fun BigDecimal.planksFromAmount(precision: Int): Balance = this.scaleByPowerOfTen(precision)
    .toBigInteger()
    .intoBalance()

fun Balance.amountFromPlanks(precision: Int) = value.toBigDecimal(scale = precision)
