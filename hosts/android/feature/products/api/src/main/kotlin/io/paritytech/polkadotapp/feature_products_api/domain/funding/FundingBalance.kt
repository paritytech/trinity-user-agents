package io.paritytech.polkadotapp.feature_products_api.domain.funding

import io.paritytech.polkadotapp.chains.network.binding.Balance

/** The payment balance the funding overlay checks a withdrawal against. */
interface FundingBalance {
    /** What the user can spend right now, or `null` when it cannot be read. */
    suspend fun spendable(): Balance?
}
