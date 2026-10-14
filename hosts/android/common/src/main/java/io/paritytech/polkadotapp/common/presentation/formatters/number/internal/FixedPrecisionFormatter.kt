package io.paritytech.polkadotapp.common.presentation.formatters.number.internal

import io.paritytech.polkadotapp.common.presentation.formatters.number.NumberFormatter
import java.math.BigDecimal
import java.math.RoundingMode

internal class FixedPrecisionFormatter(
    private val precision: Int,
    private val minPrecision: Int,
) : NumberFormatter {
    override fun format(number: BigDecimal, roundingMode: RoundingMode): String {
        val delegate = decimalFormatterFor(patternWith(precision, minPrecision), roundingMode)

        return delegate.format(number)
    }
}
