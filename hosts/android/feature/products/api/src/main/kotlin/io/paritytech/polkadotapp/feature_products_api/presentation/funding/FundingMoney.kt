package io.paritytech.polkadotapp.feature_products_api.presentation.funding

import java.math.BigDecimal
import java.math.RoundingMode
import java.text.DecimalFormat
import java.text.DecimalFormatSymbols
import java.util.Locale

/**
 * The balance as every funding screen writes it: "$1,000.25 CASH", with cents only when there are any. Sub-cent
 * amounts round down, so a balance never reads as more than can be spent; a cost rounds up, so a fee never reads as
 * less than it is.
 */
object FundingMoney {
    /** "$1,000.25". */
    fun figure(value: BigDecimal): String = "$" + format(value, RoundingMode.DOWN).format(value)

    /** "$1,000.25 CASH". */
    fun label(value: BigDecimal, symbol: String): String = "${figure(value)} $symbol"

    /** A fee or other cost, "$0.01 CASH" for anything above zero and under a cent. */
    fun cost(value: BigDecimal, symbol: String): String = "$" + format(value, RoundingMode.UP).format(value) + " $symbol"

    private fun format(value: BigDecimal, rounding: RoundingMode): DecimalFormat {
        val hasCents = value.stripTrailingZeros().scale() > 0
        return DecimalFormat("#,##0", DecimalFormatSymbols(Locale.US)).apply {
            minimumFractionDigits = if (hasCents) 2 else 0
            maximumFractionDigits = 2
            roundingMode = rounding
        }
    }
}
