package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import java.util.Locale

/** A token symbol in the token list. */
data class FundingToken(val symbol: String) {
    val monogram: String
        get() = symbol.take(1).uppercase(Locale.ROOT)
}
