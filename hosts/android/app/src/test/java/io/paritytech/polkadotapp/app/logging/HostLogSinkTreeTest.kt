package io.paritytech.polkadotapp.app.logging

import android.util.Log
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.data.telemetry.HostLogSink
import org.junit.After
import org.junit.Before
import org.junit.Test
import timber.log.Timber

class HostLogSinkTreeTest {
    private val sink = mockk<HostLogSink>(relaxed = true)

    @Before
    fun plant() {
        Timber.plant(HostLogSinkTree(sink))
    }

    @After
    fun uproot() {
        Timber.uprootAll()
    }

    @Test
    fun `a truapi line reaches the sink with its tag`() {
        Timber.tag("truapi.chain").i("chain connection %d dialling %s", 1, "wss://node.example")

        verify(exactly = 1) {
            sink.log(Log.INFO, "truapi.chain", "chain connection 1 dialling wss://node.example", emptyMap())
        }
    }

    @Test
    fun `every forwarded line keeps its own priority`() {
        Timber.tag("truapi.chain").w("chain connection 1 closing")
        Timber.tag("truapi.chain").e("chain connection 1 failed")
        Timber.tag("truapi.chain").wtf("chain connection 1 lost")

        verify(exactly = 1) {
            sink.log(Log.WARN, "truapi.chain", "chain connection 1 closing", emptyMap())
            sink.log(Log.ERROR, "truapi.chain", "chain connection 1 failed", emptyMap())
            sink.log(Log.ASSERT, "truapi.chain", "chain connection 1 lost", emptyMap())
        }
    }

    @Test
    fun `other tags and debug lines stay on the device`() {
        Timber.tag("BearerTokenAuthenticator").w("refresh failed")
        Timber.tag("truapi.core").d("detail")

        verify(exactly = 0) { sink.log(any(), any(), any(), any()) }
    }

    @Test
    fun `a looping tag sends a bounded number of lines and then counts what it dropped`() {
        var now = 0L
        Timber.uprootAll()
        Timber.plant(HostLogSinkTree(sink) { now })
        val limit = HostLogSinkTree.LINES_PER_WINDOW

        repeat(limit + 500) { Timber.tag("truapi.product").w("chainHead follow p:$it restarted 1 ms after the stop") }
        Timber.tag("truapi.chain").i("chain connection 2 open")

        verify(exactly = limit) { sink.log(Log.WARN, "truapi.product", any(), emptyMap()) }
        verify(exactly = 1) { sink.log(Log.INFO, "truapi.chain", "chain connection 2 open", emptyMap()) }

        now += HostLogSinkTree.WINDOW_MS
        Timber.tag("truapi.product").w("chainHead follow p:9999 restarted 1 ms after the stop")

        verify(exactly = 1) {
            sink.log(
                Log.WARN,
                "truapi.product",
                "500 truapi.product lines dropped over the limit of $limit a minute",
                mapOf(HostLogSinkTree.DROPPED_ATTRIBUTE to 500),
            )
            sink.log(Log.WARN, "truapi.product", "chainHead follow p:9999 restarted 1 ms after the stop", emptyMap())
        }
    }

    @Test
    fun `a window that dropped nothing sends no count`() {
        var now = 0L
        Timber.uprootAll()
        Timber.plant(HostLogSinkTree(sink) { now })

        Timber.tag("truapi.product").w("first")
        now += HostLogSinkTree.WINDOW_MS
        Timber.tag("truapi.product").w("second")

        verify(exactly = 2) { sink.log(any(), any(), any(), any()) }
    }
}
