package io.parity.truapi

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.util.UUID
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The durable engine through the shipped `.so`: the host learns whether
 * durable work is pending as soon as the runtime exists, which is what lets
 * it start recovery after a launch, and recovery returns once nothing is live.
 */
@RunWith(AndroidJUnit4::class)
class DurableRecoveryTest {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val directory = context.noBackupFilesDir.resolve("truapi-test-${UUID.randomUUID()}")

    @After
    fun cleanUp() {
        directory.deleteRecursively()
    }

    @Test
    fun aFreshLedgerReportsNoPendingWorkAndRecoveryReturnsAtOnce() {
        directory.mkdirs()
        val reported = LinkedBlockingQueue<Boolean>()
        val bridge = InertHostBridge(context, onDurableWork = reported::put)
        val runtime = TrUAPIHostRuntime(bridge, instrumentedRuntimeConfig(directory.absolutePath))

        runtime.use {
            assertEquals(false, reported.poll(5, TimeUnit.SECONDS))
            runBlocking { it.runDurableRecovery() }
        }
    }
}
