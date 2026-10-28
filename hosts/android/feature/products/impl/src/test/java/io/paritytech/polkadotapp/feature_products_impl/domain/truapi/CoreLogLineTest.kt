package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.util.Log
import io.mockk.confirmVerified
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

    /** Info lines go to Sentry, and a push notification body must never leave the device. */
    @Test
    fun `only allowed core markers are logged at info`() {
        logCoreLine("truapi.native.callback.push_notification", "hello")
        logCoreLine("truapi.tracing.warn", "truapi::runtime: chain call failed")

        verify(exactly = 1) {
            tree.log(Log.DEBUG, "truapi.core", "truapi.native.callback.push_notification: hello", null)
            tree.log(Log.INFO, "truapi.core", "truapi.tracing.warn: truapi::runtime: chain call failed", null)
        }
        // Without this a regression that *also* logged the notification line at info would pass,
        // which is the leak this test exists to catch.
        confirmVerified(tree)
    }

    /**
     * The markers the core addresses to a host are the reason it raises them. Reaching only the
     * device log would make marking an event for the host cost it the reach an ordinary warning
     * already has.
     */
    @Test
    fun `core markers addressed to the host are logged at info`() {
        logCoreLine("truapi.chain.follow_budget_reached", "four of four")
        logCoreLine("truapi.core.started", "truapi 0.0.0")
        logCoreLine("truapi.ws_bridge.connection_open", "one of eight")

        verify(exactly = 1) {
            tree.log(Log.INFO, "truapi.core", "truapi.chain.follow_budget_reached: four of four", null)
            tree.log(Log.INFO, "truapi.core", "truapi.core.started: truapi 0.0.0", null)
            tree.log(Log.INFO, "truapi.core", "truapi.ws_bridge.connection_open: one of eight", null)
        }
        confirmVerified(tree)
    }
}
