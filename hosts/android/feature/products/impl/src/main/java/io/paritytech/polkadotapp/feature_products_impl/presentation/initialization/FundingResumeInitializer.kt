package io.paritytech.polkadotapp.feature_products_impl.presentation.initialization

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingRuntime
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.truapiDatabaseDirectory
import kotlinx.coroutines.launch
import uniffi.truapi.fundingResumePending
import javax.inject.Inject

/**
 * Starts the core at launch when a provider still worked on a funding session as the app last ran, as the core
 * left a marker for in its database directory.
 *
 * The core resumes funding when it starts: it reloads the sessions, takes their providers' workers and follows their
 * top-ups and payments again. Left to a screen or product to start the core, a session could sit where it stopped
 * until the user happened to open one.
 */
class FundingResumeInitializer @Inject constructor(
    private val runtime: FundingRuntime,
    @param:ApplicationContext private val context: Context,
) : AppInitializer {
    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> {
        scope.launch {
            if (fundingResumePending(truapiDatabaseDirectory(context).absolutePath)) {
                runtime.start().logFailure("Failed to start the core to resume funding")
            }
        }

        return Result.success(Unit)
    }
}
