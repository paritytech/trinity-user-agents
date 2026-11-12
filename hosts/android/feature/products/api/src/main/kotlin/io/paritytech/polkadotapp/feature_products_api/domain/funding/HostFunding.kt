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
     * The sessions in flight and the ended ones the core keeps, as the core lists them. Observing it starts the core;
     * a build that offers no funding answers none and leaves the core alone.
     */
    fun observeActivity(): Flow<FundingActivity>

    /**
     * Records that the user saw how the ended sessions [intents] went. Call it only while the history shows them; until
     * then the core keeps each one owed to the host.
     */
    suspend fun acknowledgeShown(intents: List<String>)

    /**
     * Shows session [intent] again: one in flight where it stands, with the provider's screen over it while the
     * provider still waits on one, and an ended one on its detail screen.
     */
    suspend fun showSession(intent: String)

    /**
     * Shows the funding screens the core asks for, a session's sheet and a provider's screen, over whatever is on
     * screen, until the caller stops. The app's main screen runs it while it is started; what the core asks meanwhile
     * waits.
     */
    suspend fun presentScreens()
}
