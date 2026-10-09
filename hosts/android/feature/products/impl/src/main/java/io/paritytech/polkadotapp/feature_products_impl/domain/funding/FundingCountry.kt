package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import java.text.Collator
import java.util.Currency
import java.util.Locale

/** A country the user can pay from, as ISO 3166-1 alpha-2, with the currency a card or bank payment there uses. */
data class FundingCountry(
    val code: String,
    val name: String,
    val currencyCode: String?,
    val currencyName: String?,
) {
    /** The regional-indicator pair for the code, which the platform draws as the country's flag. */
    val flag: String
        get() = buildString {
            code.forEach { appendCodePoint(REGIONAL_INDICATOR_OFFSET + it.code) }
        }

    private companion object {
        const val REGIONAL_INDICATOR_OFFSET = 127_397
    }
}

object FundingCountries {
    fun all(locale: Locale = Locale.getDefault()): List<FundingCountry> {
        val collator = Collator.getInstance(locale)
        return Locale.getISOCountries()
            .mapNotNull { country(it, locale) }
            .sortedWith { left, right -> collator.compare(left.name, right.name) }
    }

    /** The region the device is set to, which the payment country starts at. */
    fun detected(locale: Locale = Locale.getDefault()): FundingCountry? =
        locale.country.takeIf(::isAlpha2)?.let { country(it, locale) }

    fun country(code: String, locale: Locale = Locale.getDefault()): FundingCountry? {
        val upper = code.uppercase(Locale.ROOT)
        if (!isAlpha2(upper)) return null

        val region = Locale.Builder().setRegion(upper).build()
        val name = region.getDisplayCountry(locale).takeIf { it.isNotBlank() && it != upper } ?: return null
        val currency = runCatching { Currency.getInstance(region) }.getOrNull()

        return FundingCountry(
            code = upper,
            name = name,
            currencyCode = currency?.currencyCode,
            currencyName = currency?.getDisplayName(locale),
        )
    }

    private fun isAlpha2(code: String): Boolean = code.length == 2 && code.all { it in 'A'..'Z' || it in 'a'..'z' }
}
