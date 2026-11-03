package io.parity.truapi.provider

import android.util.Log
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.truapi_provider.ChainCloseReason
import uniffi.truapi_provider.ChainLifecycle
import uniffi.truapi_provider.ChainLifecycleListener
import uniffi.truapi_provider.ChainMessageListener
import uniffi.truapi_provider.ChainPhase
import uniffi.truapi_provider.ChainProvider
import java.util.concurrent.CountDownLatch
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit

private const val TAG = "PreviewnetSmoke"
private const val RELAY = "860145753657e73c29b9388ffa0a8aebc643ea87434b4b271b6c3c3cc9e6bf92"
private const val PEOPLE = "55e3e689ecfa9d2fffcf7d309b8011956671493982230bfd0420c683542249e9"
private const val PEOPLE_RPC = "wss://previewnet.substrate.dev/people"
private const val GET_GENESIS = """{"jsonrpc":"2.0","id":1,"method":"chain_getBlockHash","params":[0]}"""

/**
 * Previewnet People through the AAR on a real device, over both transports: the embedded light
 * client and a remote node over `wss`, which goes through the bundled TLS roots. Needs network access.
 */
@RunWith(AndroidJUnit4::class)
class PreviewnetSmokeTest {

    @Test
    fun lightClientAnswersOnceTheRelayIsReady() {
        ChainProvider().use { provider ->
            val listener = Collector("light")
            val relayReady = CountDownLatch(1)

            provider.connect(PEOPLE.hexToBytes(), listener).use { connection ->
                provider.watchLifecycle(RELAY.hexToBytes(), object : ChainLifecycleListener {
                    override fun onLifecycle(state: ChainLifecycle) {
                        Log.i(TAG, "relay: $state")
                        if (state.phase is ChainPhase.Ready && state.peers > 0u) relayReady.countDown()
                    }

                    override fun onEnded() = Unit
                }).use {
                    assertTrue("relay not Ready with peers within 5 min", relayReady.await(5, TimeUnit.MINUTES))

                    connection.send(GET_GENESIS)
                    assertEquals("0x$PEOPLE", listener.awaitResponse(1, 2, TimeUnit.MINUTES).getString("result"))

                    // The genesis hash is in the bundled spec; the runtime version has to come from peers.
                    connection.send("""{"jsonrpc":"2.0","id":2,"method":"state_getRuntimeVersion","params":[]}""")
                    val version = listener.awaitResponse(2, 2, TimeUnit.MINUTES).getJSONObject("result")
                    assertTrue(version.getString("specName").isNotEmpty())
                    assertTrue(version.getInt("specVersion") > 0)
                }

                connection.disconnect()
                assertTrue(listener.awaitClosed() is ChainCloseReason.StreamEnded)
            }
        }
    }

    @Test
    fun rpcNodeAnswersOverWss() {
        ChainProvider().use { provider ->
            val listener = Collector("rpc")

            provider.connectRpc(PEOPLE_RPC, listener).use { connection ->
                connection.send(GET_GENESIS)

                assertEquals("0x$PEOPLE", listener.awaitResponse(1, 30, TimeUnit.SECONDS).getString("result"))

                connection.disconnect()
                assertTrue(listener.awaitClosed() is ChainCloseReason.StreamEnded)
            }
        }
    }

    @Test
    fun rpcNodeDeliversSubscriptionNotifications() {
        ChainProvider().use { provider ->
            val listener = Collector("rpc-sub")

            provider.connectRpc(PEOPLE_RPC, listener).use { connection ->
                connection.send("""{"jsonrpc":"2.0","id":1,"method":"chain_subscribeNewHeads","params":[]}""")
                val subscription = listener.awaitResponse(1, 30, TimeUnit.SECONDS).getString("result")

                val notification = listener.awaitNotification("chain_newHead", 60, TimeUnit.SECONDS)

                val params = notification.getJSONObject("params")
                assertEquals(subscription, params.getString("subscription"))
                assertTrue(params.getJSONObject("result").getString("number").startsWith("0x"))
                connection.disconnect()
            }
        }
    }

    private class Collector(private val label: String) : ChainMessageListener {
        private val messages = LinkedBlockingQueue<JSONObject>()
        private val closed = LinkedBlockingQueue<ChainCloseReason>()

        override fun onMessage(message: String) {
            Log.i(TAG, "$label message: ${message.take(300)}")
            messages.put(JSONObject(message))
        }

        override fun onClosed(reason: ChainCloseReason) {
            Log.i(TAG, "$label closed: $reason")
            closed.put(reason)
        }

        fun awaitResponse(id: Int, timeout: Long, unit: TimeUnit): JSONObject =
            awaitMessage(timeout, unit, "response $id") { it.optInt("id", -1) == id }

        fun awaitNotification(method: String, timeout: Long, unit: TimeUnit): JSONObject =
            awaitMessage(timeout, unit, method) { it.optString("method") == method }

        fun awaitClosed(): ChainCloseReason {
            val reason = closed.poll(30, TimeUnit.SECONDS)
            assertNotNull("$label: onClosed not called within 30 s", reason)
            return reason!!
        }

        private fun awaitMessage(timeout: Long, unit: TimeUnit, what: String, matches: (JSONObject) -> Boolean): JSONObject {
            val deadline = System.nanoTime() + unit.toNanos(timeout)
            while (true) {
                val left = deadline - System.nanoTime()
                val message = if (left > 0) messages.poll(left, TimeUnit.NANOSECONDS) else null
                assertNotNull("$label: no $what within $timeout $unit", message)
                if (matches(message!!)) return message
            }
        }
    }
}

private fun String.hexToBytes(): ByteArray = chunked(2).map { it.toInt(16).toByte() }.toByteArray()
