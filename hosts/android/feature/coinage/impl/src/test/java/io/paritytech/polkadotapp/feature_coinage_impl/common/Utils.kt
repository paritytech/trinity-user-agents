package io.paritytech.polkadotapp.feature_coinage_impl.common

import io.paritytech.polkadotapp.feature_coinage_impl.domain.common.RealCoinageBalanceConversionContext

fun Double.centsToDollar() = (this / 100).toBigDecimal()

val coinageTestPrecision = 18

val coinageTestUnits = 10000000000000000.toBigInteger()

val testConversionContext = RealCoinageBalanceConversionContext(coinageTestUnits, coinageTestPrecision)
