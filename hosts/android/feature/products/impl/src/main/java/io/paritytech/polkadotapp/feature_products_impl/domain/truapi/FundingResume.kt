package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingRuntime
import uniffi.truapi.fundingResumePending
import javax.inject.Inject

/**
 * Resumes funding a provider still worked on as the app last ran, which the core marks in its database directory.
 *
 * The core resumes funding when it starts: it reloads the sessions, takes their providers' workers and follows their
 * top-ups and payments again. Left to a screen or product to start the core, a session could sit where it stopped
 * until the user happened to open one.
 */
class FundingResume @Inject constructor(
    private val runtime: FundingRuntime,
    @param:ApplicationContext private val context: Context,
) {
    suspend fun resumeUnfinished() {
        if (fundingResumePending(truapiDatabaseDirectory(context).absolutePath)) {
            runtime.start().logFailure("Failed to start the core to resume funding")
        }
    }
}
