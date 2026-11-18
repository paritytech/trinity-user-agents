package io.paritytech.polkadotapp.feature_products_impl.domain.funding

interface FundingSessionScreens {
    /** Shows what the core asks of the app's screens until the caller stops; what it asks meanwhile waits. */
    suspend fun present()

    /** Shows session [sessionId] where it stands, or its detail once the core has ended it. */
    suspend fun show(sessionId: FundingSessionId)
}
