package io.parity.truapi

import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString
import okio.ByteString.Companion.toByteString
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.truapi.ProductId
import uniffi.truapi.ProductRendererRenderRequest
import uniffi.truapi.RenderContext
import java.util.UUID
import java.util.concurrent.TimeUnit

@RunWith(AndroidJUnit4::class)
class WorkerReferenceTest {
    private val context = InstrumentationRegistry.getInstrumentation().targetContext
    private val directory = context.noBackupFilesDir.resolve("worker-${UUID.randomUUID()}")

    @After
    fun cleanUp() {
        directory.deleteRecursively()
    }

    @Test
    fun chatAndPocketOwnTheWorkerWhileARenderCanStillBeOpenAtLastRelease() = runBlocking {
        directory.mkdirs()
        val product = ProductId.parse("worker.paseo").getOrThrow()
        val bridge = InertHostBridge(context)
        val workers = InstrumentedWorkerHost()
        TrUAPIHostRuntime(bridge, instrumentedRuntimeConfig(directory.absolutePath), workers).use { runtime ->
            val chat = runtime.claimWorker(product)
            val pocket = runtime.claimWorker(product)
            val attempt = workers.awaitAttempt()
            attempt.openExecution(bridge, null, null).use { execution ->
                val endpoint = execution.startWsBridge()
                val received = Channel<ByteString>(Channel.UNLIMITED)
                val client = OkHttpClient()
                val request = Request.Builder().url("ws://127.0.0.1:${endpoint.port}/?t=${endpoint.token}").build()
                val socket = client.newWebSocket(request, object : WebSocketListener() {
                    override fun onOpen(webSocket: WebSocket, response: Response) {
                        // SCALE request id "p", system.handshake, protocol version 2.
                        webSocket.send(byteArrayOf(4, 112, 1, 0, 0, 0, 2).toByteString())
                    }

                    override fun onMessage(webSocket: WebSocket, bytes: ByteString) {
                        received.trySend(bytes)
                    }

                    override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                        received.close(t)
                    }
                })
                try {
                    withTimeout(10_000) {
                        received.receive()
                        assertTrue(execution.reportWorkerModuleStarted())
                        val face = execution.render(ProductRendererRenderRequest(RenderContext.PocketCard("loyalty"), byteArrayOf()))
                        val first = launch { face.collect {} }
                        received.receive()
                        first.cancelAndJoin()
                        received.receive()
                        val render = launch { face.collect {} }
                        received.receive()
                        pocket.release()
                        pocket.release()
                        assertFalse(workers.stopped.isDone)
                        chat.release()
                        assertEquals(attempt.id(), workers.stopped.get(10, TimeUnit.SECONDS))
                        render.cancelAndJoin()
                    }
                } finally {
                    socket.cancel()
                    client.dispatcher.executorService.shutdown()
                }
            }
        }
    }
}
