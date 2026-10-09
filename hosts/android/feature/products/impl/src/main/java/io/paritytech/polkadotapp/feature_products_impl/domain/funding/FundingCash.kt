package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import uniffi.truapi.FundingDirection
import uniffi.truapi.U128
import java.math.BigDecimal
import java.math.BigInteger
import java.math.RoundingMode
import java.text.DecimalFormat
import java.text.DecimalFormatSymbols
import java.util.Locale

/**
 * The payment balance as the funding screens show it. Amounts cross the FFI as decimal strings of the
 * asset's smallest unit, and are drawn as dollars followed by the asset's symbol.
 */
data class FundingCash(
    val symbol: String,
    val precision: Int,
) {
    fun decimal(units: U128): BigDecimal =
        units.toBigIntegerOrNull()?.toBigDecimal()?.movePointLeft(precision)?.canonical() ?: BigDecimal.ZERO

    /** Rounds down, so the host never asks for more than the user typed. */
    fun units(value: BigDecimal): U128 =
        value.movePointRight(precision).setScale(0, RoundingMode.DOWN).toBigInteger().max(BigInteger.ZERO).toString()

    /** "$50.20", with cents only when there are any. */
    fun figure(value: BigDecimal): String = "$" + figureFormat(value).format(value)

    /** "$50.20 CASH". */
    fun label(value: BigDecimal): String = "${figure(value)} $symbol"

    /** "+$50 CASH" for value moving in, "-$50 CASH" for value moving out. */
    fun signed(value: BigDecimal, direction: FundingDirection): String {
        val sign = if (direction == FundingDirection.IN) "+" else "-"
        return sign + label(value)
    }

    private fun figureFormat(value: BigDecimal): DecimalFormat {
        val hasCents = value.stripTrailingZeros().scale() > 0
        return DecimalFormat("#,##0", DecimalFormatSymbols(Locale.US)).apply {
            minimumFractionDigits = if (hasCents) 2 else 0
            maximumFractionDigits = 2
            roundingMode = RoundingMode.HALF_EVEN
        }
    }
}

/** One scale per value, so equal amounts compare equal as data. */
fun BigDecimal.canonical(): BigDecimal = stripTrailingZeros().let { if (it.scale() < 0) it.setScale(0) else it }
