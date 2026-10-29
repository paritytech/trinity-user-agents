package io.paritytech.polkadotapp.feature_products_api.domain.funding

import io.paritytech.polkadotapp.chains.network.binding.Balance
import kotlinx.coroutines.flow.Flow

/** Which way value moves between the user's balance and everything outside it. */
enum class FundingDirection {
    IN,
    OUT,
}

/** The host's own funding entry points, served by the TrUAPI core. */
interface HostFunding {
    /**
     * Opens a funding session on the host's behalf and shows the funding overlay. Answers the session id, or
     * `null` when the user dismissed the overlay. A `null` [amount] lets the user choose.
     */
    suspend fun openFunding(direction: FundingDirection, amount: Balance?): Result<String?>

    /**
     * The sessions in flight and the ended ones the host keeps. Observing it also hands each ended session to
     * the host's store and lets the core drop it.
     */
    fun observeActivity(): Flow<FundingActivity>

    /**
     * Shows session [intent] again: one in flight where it stands, with the provider's screen over it while the
     * provider still waits on one, and an ended one on its detail screen.
     */
    suspend fun showSession(intent: String)
}
