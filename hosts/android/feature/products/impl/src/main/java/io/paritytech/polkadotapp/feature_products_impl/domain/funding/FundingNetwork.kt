package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import java.util.Locale

/**
 * A crypto network id from a provider's routes. The ids are the lowercase names the core uses (`polkadot` is
 * Asset Hub); an id this table does not know is shown capitalised.
 */
data class FundingNetwork(val id: String) {
    val name: String
        get() = NAMES[id] ?: id.replaceFirstChar { it.titlecase(Locale.ROOT) }

    val monogram: String
        get() = name.take(1).uppercase(Locale.ROOT)

    private companion object {
        val NAMES = mapOf(
            "polkadot" to "Polkadot",
            "bitcoin" to "Bitcoin",
            "ethereum" to "Ethereum",
            "tron" to "Tron",
            "solana" to "Solana",
        )
    }
}

/** A token symbol in the token list. */
data class FundingToken(val symbol: String) {
    val monogram: String
        get() = symbol.take(1).uppercase(Locale.ROOT)
}
