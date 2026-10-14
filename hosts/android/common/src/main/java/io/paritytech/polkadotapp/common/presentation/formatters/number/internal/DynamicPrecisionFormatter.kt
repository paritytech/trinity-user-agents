package io.paritytech.polkadotapp.common.presentation.formatters.number.internal

import io.paritytech.polkadotapp.common.presentation.formatters.number.NumberFormatter
import java.lang.Integer.max
import java.math.BigDecimal
import java.math.RoundingMode
import kotlin.math.min

internal class DynamicPrecisionFormatter(
    private val minScale: Int,
    private val minPrecision: Int,
    private val padTrailingZeros: Boolean
) : NumberFormatter {
    override fun format(number: BigDecimal, roundingMode: RoundingMode): String {
        // scale() - total amount of digits after 0.,
        // precision() - amount of non-zero digits in decimal part
        val zeroPrecision = number.scale() - number.precision()
        val requiredPrecision = zeroPrecision + min(number.precision(), minPrecision)

        val formattingPrecision = max(minScale, requiredPrecision)
        val mandatoryDecimals = if (padTrailingZeros) minScale else 0

        return decimalFormatterFor(patternWith(formattingPrecision, mandatoryDecimals), roundingMode).format(number)
    }
}
