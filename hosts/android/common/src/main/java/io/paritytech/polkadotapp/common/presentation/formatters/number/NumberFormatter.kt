package io.paritytech.polkadotapp.common.presentation.formatters.number

import java.math.BigDecimal
import java.math.BigInteger
import java.math.RoundingMode

interface NumberFormatter {
    fun format(number: BigDecimal, roundingMode: RoundingMode = RoundingMode.DOWN): String
}

fun NumberFormatter.format(number: Int, roundingMode: RoundingMode = RoundingMode.DOWN): String {
    return format(number.toBigDecimal(), roundingMode)
}

fun NumberFormatter.format(number: BigInteger, roundingMode: RoundingMode = RoundingMode.DOWN): String {
    return format(number.toBigDecimal(), roundingMode)
}
