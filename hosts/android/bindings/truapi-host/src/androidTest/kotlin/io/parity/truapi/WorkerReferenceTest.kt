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
import org.junit.Test
import org.junit.runner.RunWith
import uniffi.truapi.ExecutableKind
import uniffi.truapi.ProductExecutionConfig
import uniffi.truapi.ProductId
import uniffi.truapi.ProductRendererRenderRequest
import uniffi.truapi.RenderContext
import uniffi.truapi.WorkerTransition
import java.util.UUID
import java.util.concurrent.CopyOnWriteArrayList

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
        val transitions = CopyOnWriteArrayList<WorkerTransition>()
        val bridge = object : HostBridge by InertHostBridge(context) {
            override fun requestWorkerTransition(productId: ProductId, transition: WorkerTransition) {
                if (productId == product) transitions += transition
            }
        }
        TrUAPIHostRuntime(bridge, instrumentedRuntimeConfig(directory.absolutePath)).use { runtime ->
            val chat = runtime.claimWorker(product)
            val pocket = runtime.claimWorker(product)
            runtime.openProductExecution(bridge, ProductExecutionConfig(product, ExecutableKind.WORKER)).use { execution ->
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
                        val face = execution.render(ProductRendererRenderRequest(RenderContext.PocketCard("loyalty"), byteArrayOf()))
                        val first = launch { face.collect {} }
                        received.receive()
                        first.cancelAndJoin()
                        received.receive()
                        val render = launch { face.collect {} }
                        received.receive()
                        pocket.release()
                        pocket.release()
                        assertEquals(listOf(WorkerTransition.START), transitions.toList())
                        chat.release()
                        assertEquals(listOf(WorkerTransition.START, WorkerTransition.STOP), transitions.toList())
                        render.cancelAndJoin()
                        assertEquals(listOf(WorkerTransition.START, WorkerTransition.STOP), transitions.toList())
                    }
                } finally {
                    socket.cancel()
                    client.dispatcher.executorService.shutdown()
                }
            }
        }
    }
}
