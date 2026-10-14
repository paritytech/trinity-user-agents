package io.paritytech.polkadotapp.feature_prices_impl.presentation.formatter

import io.paritytech.polkadotapp.common.presentation.formatters.number.NumberFormatterFactory
import io.paritytech.polkadotapp.common.presentation.formatters.number.NumberFormatterFactory.Params
import io.paritytech.polkadotapp.common.presentation.formatters.number.NumberFormatterFactory.Params.KnownAbbreviation
import io.paritytech.polkadotapp.common.presentation.formatters.number.NumberFormatterFactory.Params.SmallNumberParams
import io.paritytech.polkadotapp.feature_prices_api.presentation.formatter.FiatFormatter
import io.paritytech.polkadotapp.feature_prices_api.presentation.mapper.model.FiatAmountModel
import java.math.RoundingMode
import javax.inject.Inject

internal class RealFiatFormatter @Inject constructor(
    numberFormatterFactory: NumberFormatterFactory
) : FiatFormatter {
    private val formatter = numberFormatterFactory.createFormatter(
        params = Params(
            includedAbbreviations = KnownAbbreviation.all(),
            unabbreviatedTotalDecimals = 2,
            unabbreviatedMinTotalDecimals = 2,
            abbreviatedTotalDecimals = 2,
            abbreviatedMinTotalDecimals = 0,
            smallNumberParams = SmallNumberParams(
                minSignificantDecimals = 1,
                minTotalDecimals = 2,
                padTrailingZeros = false
            ),
            roundingMode = RoundingMode.DOWN
        )
    )

    override fun formatFiatAmount(
        fiatAmount: FiatAmountModel,
        approx: Boolean
    ): String {
        return buildString {
            if (approx) append("≈ ")

            append(fiatAmount.currencyDisplay)
            append(formatter.format(fiatAmount.fiatAmount))
        }
    }
}
