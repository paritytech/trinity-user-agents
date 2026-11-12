package io.paritytech.polkadotapp.common.utils

import io.paritytech.polkadotapp.common.BuildConfig

object CurrencyConfig {
    val defaultSymbol: String = BuildConfig.CURRENCY_SYMBOL
    val fiatSymbol: String = BuildConfig.FIAT_SYMBOL
}
