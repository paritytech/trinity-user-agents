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

    /** Info lines go to Sentry, and a push notification body must never leave the device. */
    @Test
    fun `only core tracing warnings and errors are logged at info`() {
        logCoreLine("truapi.native.callback.push_notification", "hello")
        logCoreLine("truapi.tracing.warn", "truapi::runtime: chain call failed")

        verify(exactly = 1) {
            tree.log(Log.DEBUG, "truapi.core", "truapi.native.callback.push_notification: hello", null)
            tree.log(Log.INFO, "truapi.core", "truapi.tracing.warn: truapi::runtime: chain call failed", null)
        }
    }
}
