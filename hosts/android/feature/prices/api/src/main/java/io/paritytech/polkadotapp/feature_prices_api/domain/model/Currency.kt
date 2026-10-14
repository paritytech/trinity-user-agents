package io.paritytech.polkadotapp.feature_prices_api.domain.model

enum class Currency(
    val id: Int,
    val code: String,
    val displayName: String,
    val symbol: String?,
    val coinId: String
) {
    USD(0, "USD", "United States Dollar", "$", "usd"),
    EUR(1, "EUR", "Euro", "€", "eur"),
    CAD(2, "CAD", "Canadian Dollar", "C$", "cad"),
    IDR(3, "IDR", "Indonesian Rupiah", "Rp", "idr");

    companion object {
        val DEFAULT = USD
    }
}

val Currency.display: String
    get() = symbol ?: code

fun Currency.Companion.fromCode(code: String?): Currency {
    return Currency.entries.find { it.code == code } ?: DEFAULT
}
