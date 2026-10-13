package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.logFailure
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import uniffi.truapi.FundingDirection
import javax.inject.Inject
import javax.inject.Singleton

/** Opens funding sessions the user comes back to from the CASH card, and new ones once a screen has gone. */
@Singleton
class FundingSessionPresenter @Inject constructor(
    private val overlay: AppFundingOverlay,
    private val runtime: FundingRuntime,
    dispatchers: CoroutineDispatchers,
) {
    // Outlives the sheet that asks for a new session, which closes before the overlay for it opens.
    private val scope = CoroutineScope(SupervisorJob() + dispatchers.computation)

    suspend fun show(intent: String) {
        overlay.reopen(intent)
    }

    /** A new session the same way, from the start. */
    fun startOver(direction: FundingDirection) {
        scope.launch { runtime.openFunding(direction).logFailure("Funding could not start over") }
    }
}
