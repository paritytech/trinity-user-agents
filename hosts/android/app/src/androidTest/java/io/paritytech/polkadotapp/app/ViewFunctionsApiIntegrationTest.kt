package io.paritytech.polkadotapp.app

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.work.testing.WorkManagerTestInitHelper
import dagger.hilt.android.testing.HiltAndroidRule
import dagger.hilt.android.testing.HiltAndroidTest
import io.novasama.substrate_sdk_android.koltinx_serialization_scale.serializers.BigIntegerSerializable
import io.paritytech.polkadotapp.chains.call.MultiChainViewFunctionsApi
import io.paritytech.polkadotapp.chains.call.call
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.connection.ChainConnectionRefCounter
import io.paritytech.polkadotapp.chains.multiNetwork.connection.withConnectionEnabled
import io.paritytech.polkadotapp.chains.util.EncodedArguments.Companion.noArgs
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import timber.log.Timber
import javax.inject.Inject

private const val LABEL = "ViewFunctionsApiIntegrationTest"

private const val PALLET = "Resources"
private const val VIEW_FUNCTION = "current_stmt_store_period"

/**
 * Exercises [MultiChainViewFunctionsApi] end-to-end against the real People chain: dispatches a v16
 * view function over `state_call` and decodes its output via kotlinx SCALE.
 *
 * This runs on the device's persisted state, not an isolated fixture. Prerequisites:
 *  - The debug app (io.paritytech.polkadotapp.debug) must have been launched and onboarded once on the same
 *    device/emulator, so the `chains` table is synced.
 *  - App initializers do not run under HiltTestApplication; the chain socket is enabled explicitly via
 *    [withConnectionEnabled].
 *
 * Not wired into CI — it depends on a live endpoint.
 */

@HiltAndroidTest
@RunWith(AndroidJUnit4::class)
class ViewFunctionsApiIntegrationTest: BaseIntegrationTest() {

    @Inject lateinit var chainRegistry: ChainRegistry
    @Inject lateinit var chainConnectionRefCounter: ChainConnectionRefCounter
    @Inject lateinit var viewFunctionsApi: MultiChainViewFunctionsApi

    @Test
    fun decodesViewFunctionOutputFromThePeopleChain() = runBlocking<Unit> {
        val chain = chainRegistry.peopleChain()

        chainConnectionRefCounter.withConnectionEnabled(chain.id, LABEL) {
            assertTrue(
                "$PALLET.$VIEW_FUNCTION is not exposed — chain is likely not on metadata v16",
                viewFunctionsApi.isSupported(chain.id, PALLET, VIEW_FUNCTION)
            )

            val period: BigIntegerSerializable = viewFunctionsApi.forChain(chain.id)
                .call<BigIntegerSerializable>(
                    pallet = PALLET,
                    name = VIEW_FUNCTION,
                    arguments = noArgs()
                )
                .getOrThrow()

            Timber.d("$PALLET.$VIEW_FUNCTION -> $period")

            assertTrue("Expected a positive statement-store period, got $period", period.signum() > 0)
        }
    }
}
