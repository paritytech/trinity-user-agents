package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.util.amountFromPlanks
import java.math.BigDecimal
import java.math.RoundingMode
import java.text.DecimalFormat
import java.text.DecimalFormatSymbols
import java.text.NumberFormat
import java.util.Currency
import java.util.Locale

data class FundingAssetUnit(
    val code: String,
    val decimals: Int,
    val isFiat: Boolean,
) {
    companion object {
        private const val CRYPTO_DISPLAY_DIGITS = 6
        private const val CODE_MINIMUM_DIGITS = 2

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

    fun decimal(units: Balance): BigDecimal = units.amountFromPlanks(decimals).canonical()

    fun format(units: Balance, locale: Locale = Locale.getDefault()): String = format(decimal(units), locale)

    fun format(value: BigDecimal, locale: Locale = Locale.getDefault()): String {
        val currency = if (isFiat) isoCurrency(code) else null
        if (currency == null) return "${plain(value, minOf(decimals, CRYPTO_DISPLAY_DIGITS))} $code"

        return NumberFormat.getCurrencyInstance(locale).apply {
            this.currency = currency
            minimumFractionDigits = decimals
            maximumFractionDigits = decimals
        }.format(value)
    }

    fun formatWithCode(units: Balance): String {
        val digits = if (isFiat) decimals else CRYPTO_DISPLAY_DIGITS
        return "${plain(decimal(units), digits, CODE_MINIMUM_DIGITS)} $code"
    }
}

// One scale per value, so equal amounts compare equal as data.
internal fun BigDecimal.canonical(): BigDecimal = stripTrailingZeros().let { if (it.scale() < 0) it.setScale(0) else it }
