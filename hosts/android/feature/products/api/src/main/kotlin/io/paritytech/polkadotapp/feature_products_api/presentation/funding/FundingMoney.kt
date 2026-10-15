package io.paritytech.polkadotapp.feature_products_api.presentation.funding

import java.math.BigDecimal
import java.math.RoundingMode
import java.text.DecimalFormat
import java.text.DecimalFormatSymbols
import java.util.Locale

/**
 * The balance as every funding screen writes it: "$1,000.25 CASH", with cents only when there are any. Sub-cent
 * amounts round down, so a balance never reads as more than can be spent.
 */
object FundingMoney {
    /** "$1,000.25". */
    fun figure(value: BigDecimal): String = "$" + format(value).format(value)

    /** "$1,000.25 CASH". */
    fun label(value: BigDecimal, symbol: String): String = "${figure(value)} $symbol"

    private fun format(value: BigDecimal): DecimalFormat {
        val hasCents = value.stripTrailingZeros().scale() > 0
        return DecimalFormat("#,##0", DecimalFormatSymbols(Locale.US)).apply {
            minimumFractionDigits = if (hasCents) 2 else 0
            maximumFractionDigits = 2
            roundingMode = RoundingMode.DOWN
        }
    }
}
