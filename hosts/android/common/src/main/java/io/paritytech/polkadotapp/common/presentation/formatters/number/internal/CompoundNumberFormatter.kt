package io.paritytech.polkadotapp.common.presentation.formatters.number.internal

import io.paritytech.polkadotapp.common.presentation.formatters.number.NumberFormatter
import java.math.BigDecimal
import java.math.MathContext
import java.math.RoundingMode

internal class CompoundNumberFormatter(
    private val abbreviations: List<NumberAbbreviation>,
) : NumberFormatter {
    class NumberAbbreviation(
        val threshold: BigDecimal,
        val divisor: BigDecimal,
        val suffix: String,
        val formatter: NumberFormatter,
    )

    init {
        require(abbreviations.isNotEmpty()) {
            "Cannot create compound formatter with empty abbreviations"
        }

        require(
            abbreviations.zipWithNext().all { (current, next) ->
                current.threshold <= next.threshold
            },
        ) {
            "Abbreviations should go in non-descending order w.r.t. threshold"
        }
    }

    override fun format(number: BigDecimal, roundingMode: RoundingMode): String {
        val lastAbbreviationMatching =
            abbreviations.lastOrNull { number >= it.threshold }
                ?: abbreviations.first()

        val scaled = number.divide(lastAbbreviationMatching.divisor, MathContext.UNLIMITED)

        return lastAbbreviationMatching.formatter.format(scaled, roundingMode) + lastAbbreviationMatching.suffix
    }
}
