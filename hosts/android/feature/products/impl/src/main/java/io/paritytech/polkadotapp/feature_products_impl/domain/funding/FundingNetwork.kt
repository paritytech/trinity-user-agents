package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import uniffi.truapi.fundingNetworkName
import java.util.Locale

/** A crypto network id from a provider's routes, with the name the core gives it. */
data class FundingNetwork(val id: String, val name: String) {
    val monogram: String
        get() = name.take(1).uppercase(Locale.ROOT)

    companion object {
        /** Network [id] by the core's name for it. */
        fun of(id: String): FundingNetwork = FundingNetwork(id, fundingNetworkName(id))
    }
}

/** A token symbol in the token list. */
data class FundingToken(val symbol: String) {
    val monogram: String
        get() = symbol.take(1).uppercase(Locale.ROOT)
}
