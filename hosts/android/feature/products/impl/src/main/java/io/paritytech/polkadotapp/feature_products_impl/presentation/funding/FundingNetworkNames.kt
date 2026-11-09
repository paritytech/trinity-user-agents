package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import java.util.Locale

private val NETWORK_NAMES = mapOf(
    "polkadot" to "Polkadot",
    "bitcoin" to "Bitcoin",
    "ethereum" to "Ethereum",
    "tron" to "Tron",
    "solana" to "Solana",
)

/** A crypto network id from a provider's routes, by name; an id this table does not know is shown capitalised. */
fun fundingNetworkName(id: String): String = NETWORK_NAMES[id] ?: id.replaceFirstChar { it.titlecase(Locale.ROOT) }

fun monogramOf(name: String): String = name.take(1).uppercase(Locale.ROOT)
