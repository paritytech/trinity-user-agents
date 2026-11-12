package io.paritytech.polkadotapp.feature_products_impl.domain.funding

/** Where funding sessions are shown: the ones the core asks the app's screens for, and the ones the user comes back to. */
interface FundingSessionScreens {
    /** Shows what the core asks of the app's screens until the caller stops; what it asks meanwhile waits. */
    suspend fun present()

    /** Shows session [intent] where it stands, or its detail once the core has ended it. */
    suspend fun show(intent: String)
}
