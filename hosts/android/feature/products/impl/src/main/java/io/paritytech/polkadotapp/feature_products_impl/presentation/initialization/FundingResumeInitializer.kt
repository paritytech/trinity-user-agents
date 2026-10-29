package io.paritytech.polkadotapp.feature_products_impl.presentation.initialization

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.AppFundingHostBridge
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.launch
import javax.inject.Inject

/**
 * Starts the core at launch when a provider still worked on a funding session as the app last ran.
 *
 * The core resumes funding when it starts: it reloads the sessions, takes their providers' workers and follows their
 * top-ups and payments again. Left to a screen or product to start the core, a session could sit where it stopped
 * until the user happened to open one. The in-progress flag is what the core last pushed: once when funding resumes,
 * and again whenever it changes, so once nothing is left to resume a launch no longer starts the core early.
 */
class FundingResumeInitializer @Inject constructor(
    private val runtimeProvider: TrUAPIHostRuntimeProvider,
    private val fundingBridge: AppFundingHostBridge,
    private val preferences: Preferences,
) : AppInitializer {
    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> {
        scope.launch {
            fundingBridge.inProgress().filterNotNull().collect { preferences.putBoolean(IN_PROGRESS, it) }
        }
        if (preferences.getBoolean(IN_PROGRESS, false)) {
            scope.launch {
                runtimeProvider.runtime().logFailure("Failed to start the core to resume funding")
            }
        }

        return Result.success(Unit)
    }

    private companion object {
        const val IN_PROGRESS = "funding_in_progress"
    }
}
