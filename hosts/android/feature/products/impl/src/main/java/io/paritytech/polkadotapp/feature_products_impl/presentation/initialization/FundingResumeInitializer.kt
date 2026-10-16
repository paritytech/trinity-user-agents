package io.paritytech.polkadotapp.feature_products_impl.presentation.initialization

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.data.storage.preferences.Preferences
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.AppFundingHostBridge
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import kotlinx.coroutines.launch
import javax.inject.Inject

/**
 * Starts the core at launch when a provider still worked on a funding session as the app last ran.
 *
 * The core resumes funding when it starts: it reloads the sessions, takes their providers' workers and follows their
 * top-ups and payments again. Left to a screen or product to start the core, a session could sit where it stopped
 * until the user happened to open one. The in-progress flag is written again once the core is up, from the sessions
 * it stored, and on every session change after that, so once nothing is left to resume a launch no longer starts
 * the core early.
 */
class FundingResumeInitializer @Inject constructor(
    private val runtimeProvider: TrUAPIHostRuntimeProvider,
    private val fundingBridge: AppFundingHostBridge,
    private val preferences: Preferences,
) : AppInitializer {
    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> {
        scope.launch {
            fundingBridge.changedSessions().collect { recordInProgress() }
        }
        if (preferences.getBoolean(IN_PROGRESS, false)) {
            scope.launch {
                runtimeProvider.runtime()
                    .logFailure("Failed to start the core to resume funding")
                    .onSuccess { recordInProgress() }
            }
        }

        return Result.success(Unit)
    }

    private suspend fun recordInProgress() {
        runtimeProvider.runtime()
            .mapCatching { runtime -> runtime.fundingInProgress() }
            .onSuccess { inProgress -> preferences.putBoolean(IN_PROGRESS, inProgress) }
            .logFailure("Failed to record whether funding is in progress")
    }

    private companion object {
        const val IN_PROGRESS = "funding_in_progress"
    }
}
