package io.paritytech.polkadotapp.common.presentation.formatters.number

import java.math.RoundingMode

class NumberFormatters(
    val default: NumberFormatter,
)

fun NumberFormatterFactory.createSharedFormatters(): NumberFormatters {
    return NumberFormatters(
        default = createDefault()
    )
}

private fun NumberFormatterFactory.createDefault(): NumberFormatter {
    return createFormatter(
        params = NumberFormatterFactory.Params(
            includedAbbreviations = NumberFormatterFactory.Params.KnownAbbreviation.all(),
            unabbreviatedTotalDecimals = 2,
            unabbreviatedMinTotalDecimals = 0,
            abbreviatedTotalDecimals = 2,
            abbreviatedMinTotalDecimals = 0,
            smallNumberParams = NumberFormatterFactory.Params.SmallNumberParams(
                minSignificantDecimals = 1,
                minTotalDecimals = 2,
                padTrailingZeros = false
            ),
            roundingMode = RoundingMode.DOWN
        )
    )
}
