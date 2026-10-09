package io.paritytech.polkadotapp.feature_products_api.domain.funding

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import kotlinx.coroutines.flow.Flow

/** Which way value moves between the user's balance and everything outside it. */
enum class FundingDirection {
    IN,
    OUT,
}

/** A funding session the core keeps. */
data class HostFundingSession(
    val intent: String,
    val direction: FundingDirection,
    val amount: Balance?,
    val providerId: ProductId?,
    val inFlight: Boolean,
)

/** The host's own funding entry points, served by the TrUAPI core. */
interface HostFunding {
    /**
     * Opens a funding session on the host's behalf and shows the funding overlay. Answers the session id, or
     * `null` when the user dismissed the overlay. A `null` [amount] lets the user choose.
     */
    suspend fun openFunding(direction: FundingDirection, amount: Balance?): Result<String?>

    /** Every session the core keeps, in flight first, re-read whenever one changes. */
    fun observeSessions(): Flow<List<HostFundingSession>>

    /**
     * The sessions in flight and the ended ones the host keeps. Observing it also hands each ended session to
     * the host's store and lets the core drop it.
     */
    fun observeActivity(): Flow<FundingActivity>
}
