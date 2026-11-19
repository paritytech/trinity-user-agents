package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.util.Log
import io.mockk.spyk
import io.mockk.verify
import org.junit.After
import org.junit.Before
import org.junit.Test
import timber.log.Timber

class CoreLogLineTest {
    private val tree = spyk(
        object : Timber.Tree() {
            public override fun log(priority: Int, tag: String?, message: String, t: Throwable?) = Unit
        },
    )

    @Before
    fun plant() {
        Timber.plant(tree)
    }

    @After
    fun uproot() {
        Timber.uprootAll()
    }

    /**
     * A local marker's detail describes what the person is doing, so it is logged below the level
     * `SentryLogTree` forwards.
     */
    @Test
    fun `a local marker stays on the device`() {
        logCoreLine("truapi.native.callback.push_notification", "hello")

        verify(exactly = 1) {
            tree.log(Log.DEBUG, "truapi.core", "truapi.native.callback.push_notification: hello", null)
        }
        // Stated as its own negative, because a notification body reaching info is the leak this
        // test exists to catch. `confirmVerified` cannot stand in for it: Timber calls
        // `getExplicitTag`, the 3-arg `log` and `getTag` on a planted tree before the call verified
        // above, so acknowledging every interaction is not something a Timber spy allows.
        verify(exactly = 0) {
            tree.log(Log.INFO, "truapi.core", "truapi.native.callback.push_notification: hello", null)
        }
    }

    /**
     * The severity comes from the core's table, so an error is alerted on separately rather than
     * arriving as one more info line.
     */
    @Test
    fun `an exported marker is logged at its own severity`() {
        logCoreLine("truapi.core.started", "truapi 0.0.0")
        logCoreLine("truapi.ws_bridge.connection_open", "one of eight")
        logCoreLine("truapi.chain.follow_budget_reached", "four of four")
        logCoreLine("truapi.tracing.warn", "truapi::runtime: retrying")
        logCoreLine("truapi.tracing.error", "truapi::runtime: chain call failed")

        verify(exactly = 1) {
            tree.log(Log.INFO, "truapi.core", "truapi.core.started: truapi 0.0.0", null)
            tree.log(Log.INFO, "truapi.core", "truapi.ws_bridge.connection_open: one of eight", null)
            tree.log(Log.WARN, "truapi.core", "truapi.chain.follow_budget_reached: four of four", null)
            tree.log(Log.WARN, "truapi.core", "truapi.tracing.warn: truapi::runtime: retrying", null)
            tree.log(Log.ERROR, "truapi.core", "truapi.tracing.error: truapi::runtime: chain call failed", null)
        }
        // The iOS twin gets this from comparing the whole recorded array. Here it is stated,
        // because a marker logged at its own level and at info as well would satisfy every
        // positive above while putting the severity decision back in two places.
        verify(exactly = 0) {
            tree.log(Log.INFO, "truapi.core", "truapi.tracing.error: truapi::runtime: chain call failed", null)
        }
    }

    /**
     * Nothing downstream knows what an unlisted marker carries, so it is treated as local rather
     * than exported on the strength of its prefix.
     */
    @Test
    fun `a marker the table does not list stays on the device`() {
        logCoreLine("truapi.chain.not_registered", "whatever this is")

        verify(exactly = 1) {
            tree.log(Log.DEBUG, "truapi.core", "truapi.chain.not_registered: whatever this is", null)
        }
    }
}
