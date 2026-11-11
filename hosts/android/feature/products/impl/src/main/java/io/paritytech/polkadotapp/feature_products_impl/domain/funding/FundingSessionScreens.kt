package io.paritytech.polkadotapp.feature_products_impl.domain.funding

/** Where a funding session the user comes back to is shown. */
interface FundingSessionScreens {
    /** Shows session [intent] where it stands, or its detail once the core has ended it. */
    suspend fun show(intent: String)
}
