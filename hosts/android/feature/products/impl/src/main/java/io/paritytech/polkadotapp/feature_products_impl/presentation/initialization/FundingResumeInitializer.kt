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
 * until the user happened to open one. Every session change keeps the answer current for the next launch.
 */
class FundingResumeInitializer @Inject constructor(
    private val runtimeProvider: TrUAPIHostRuntimeProvider,
    private val fundingBridge: AppFundingHostBridge,
    private val preferences: Preferences,
) : AppInitializer {
    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> {
        scope.launch {
            fundingBridge.changedSessions().collect {
                runtimeProvider.runtime()
                    .map { runtime -> preferences.putBoolean(IN_PROGRESS, runtime.fundingInProgress()) }
                    .logFailure("Failed to record whether funding is in progress")
            }
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
