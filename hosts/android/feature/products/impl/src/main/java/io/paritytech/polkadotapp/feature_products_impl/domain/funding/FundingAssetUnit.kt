package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import uniffi.truapi.U128
import java.math.BigDecimal
import java.math.RoundingMode
import java.text.DecimalFormat
import java.text.DecimalFormatSymbols
import java.text.NumberFormat
import java.util.Currency
import java.util.Locale

/**
 * What a quote's `send_amount` (In) or `receive_amount` (Out) is counted in: the asset the ask named, in its
 * smallest unit, at the decimals the provider stated with the quote or the deposit. A code ISO 4217 knows is
 * shown as money.
 */
data class FundingAssetUnit(
    val code: String,
    val decimals: Int,
    val isFiat: Boolean,
) {
    companion object {
        private const val CRYPTO_DISPLAY_DIGITS = 6
        private const val CODE_MINIMUM_DIGITS = 2

        /** A unit at the decimals the provider stated with a quote or a deposit instruction. */
        fun of(code: String, decimals: Int): FundingAssetUnit {
            val upper = code.uppercase(Locale.ROOT)
            return FundingAssetUnit(upper, decimals, isFiat = isoCurrency(upper) != null)
        }

        private fun isoCurrency(code: String): Currency? = runCatching { Currency.getInstance(code) }.getOrNull()

        private fun plain(value: BigDecimal, maximumDigits: Int, minimumDigits: Int = 0): String =
            DecimalFormat("#,##0", DecimalFormatSymbols(Locale.US)).apply {
                minimumFractionDigits = minOf(minimumDigits, maximumDigits)
                maximumFractionDigits = maximumDigits
                roundingMode = RoundingMode.HALF_EVEN
            }.format(value)
    }

    fun decimal(units: U128): BigDecimal =
        units.toBigIntegerOrNull()?.toBigDecimal()?.movePointLeft(decimals)?.canonical() ?: BigDecimal.ZERO

    /** "€50.05" for fiat, "51.80 USDT" for crypto. */
    fun format(units: U128, locale: Locale = Locale.getDefault()): String = format(decimal(units), locale)

    fun format(value: BigDecimal, locale: Locale = Locale.getDefault()): String {
        val currency = if (isFiat) isoCurrency(code) else null
        if (currency == null) return "${plain(value, minOf(decimals, CRYPTO_DISPLAY_DIGITS))} $code"

        return NumberFormat.getCurrencyInstance(locale).apply {
            this.currency = currency
            minimumFractionDigits = decimals
            maximumFractionDigits = decimals
        }.format(value)
    }

    /** "25.10 EUR", the way the provider list sets a quote. */
    fun formatWithCode(units: U128): String {
        val digits = if (isFiat) decimals else CRYPTO_DISPLAY_DIGITS
        return "${plain(decimal(units), digits, CODE_MINIMUM_DIGITS)} $code"
    }
}
