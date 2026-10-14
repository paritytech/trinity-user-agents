package io.paritytech.polkadotapp.common.presentation.formatters.number.internal

import java.math.RoundingMode
import java.text.DecimalFormat

private const val DECIMAL_PATTERN_BASE = "###,##0."

private const val GROUPING_SEPARATOR = ','
private const val DECIMAL_SEPARATOR = '.'

internal fun patternWith(precision: Int, minPrecision: Int): String {
    require(minPrecision in 0..precision) { "minPrecision=$minPrecision must be in 0..$precision" }
    val mandatory = "0".repeat(minPrecision)
    val optional = "#".repeat(precision - minPrecision)
    return "$DECIMAL_PATTERN_BASE$mandatory$optional"
}

internal fun decimalFormatterFor(
    pattern: String,
    roundingMode: RoundingMode,
): DecimalFormat {
    return DecimalFormat(pattern).apply {
        val symbols = decimalFormatSymbols

        symbols.groupingSeparator = GROUPING_SEPARATOR
        symbols.decimalSeparator = DECIMAL_SEPARATOR

        decimalFormatSymbols = symbols

        this.roundingMode = roundingMode
        decimalFormatSymbols = decimalFormatSymbols
    }
}
