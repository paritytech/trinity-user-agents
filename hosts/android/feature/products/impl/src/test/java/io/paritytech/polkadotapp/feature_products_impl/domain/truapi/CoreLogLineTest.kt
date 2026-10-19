package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.util.Log
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import timber.log.Timber

class CoreLogLineTest {
    private data class Line(val priority: Int, val tag: String?, val message: String)

    private val lines = mutableListOf<Line>()

    private val recorder = object : Timber.Tree() {
        override fun log(priority: Int, tag: String?, message: String, t: Throwable?) {
            lines += Line(priority, tag, message)
        }
    }

    @Before
    fun plant() {
        Timber.plant(recorder)
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

        assertEquals(
            listOf(
                Line(Log.DEBUG, "truapi.core", "truapi.native.callback.push_notification: hello"),
                Line(Log.INFO, "truapi.core", "truapi.tracing.warn: truapi::runtime: chain call failed"),
            ),
            lines,
        )
    }
}
