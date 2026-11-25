package io.paritytech.polkadotapp.chains.multiNetwork.connection.provider

import com.google.gson.Gson
import com.google.gson.JsonObject
import com.neovisionaries.ws.client.WebSocketAdapter
import com.neovisionaries.ws.client.WebSocketState
import io.mockk.mockk
import io.novasama.substrate_sdk_android.wsrpc.SocketService
import io.novasama.substrate_sdk_android.wsrpc.executeAsync
import io.novasama.substrate_sdk_android.wsrpc.networkStateFlow
import io.novasama.substrate_sdk_android.wsrpc.recovery.Reconnector
import io.novasama.substrate_sdk_android.wsrpc.request.RequestExecutor
import io.novasama.substrate_sdk_android.wsrpc.request.runtime.RuntimeRequest
import io.novasama.substrate_sdk_android.wsrpc.state.SocketStateMachine.State
import io.novasama.substrate_sdk_android.wsrpc.subscriptionFlow
import io.parity.truapi.LightClient
import io.paritytech.polkadotapp.common.data.network.WsConnectionLogger
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.concurrent.CountDownLatch
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import kotlin.time.Duration.Companion.seconds
import com.neovisionaries.ws.client.WebSocket as SdkWebSocket

class ProviderWebSocketTest {
    private val gson = Gson()
    private val server = MockWebServer()
    private var cleartextPermitted = true
    private val socketService = SocketService(
        jsonMapper = gson,
        logger = WsConnectionLogger(ignoreStateMachineLogs = true),
        webSocketFactory = ProviderWebSocketFactory.forNodes(ByteArray(GENESIS_SIZE)) { cleartextPermitted },
        reconnector = Reconnector(),
        requestExecutor = RequestExecutor(),
    )

    @After
    fun tearDown() {
        socketService.stop()
        server.close()
    }

    @Test
    fun `a request is answered by the node`() = runBlocking {
        socketService.start(serve(node()), remainPaused = false)

        val response = withTimeout(TIMEOUT) { socketService.executeAsync(RuntimeRequest("system_health", emptyList())) }

        assertEquals("system_health", response.result)
    }

    // The provider never reconnects, so a node dropping the socket must reach SocketService
    // as a closed socket: only then does it dial again and replay its subscriptions.
    @Test
    fun `a subscription resumes on a new socket after the node drops the old one`() = runBlocking {
        val url = serve(node(dropAfterNotifying = true))
        server.enqueue(upgrade(node()))
        socketService.start(url, remainPaused = false)

        val heads = withTimeout(TIMEOUT) {
            socketService.subscriptionFlow(RuntimeRequest(SUBSCRIBE, emptyList()))
                .map { change -> change.params.result }
                .take(2)
                .toList()
        }

        assertEquals(listOf(HEAD, HEAD), heads)
        assertEquals(2, server.requestCount)
    }

    // The provider's sockets bypass the app's network security config, so a release build
    // that forbids cleartext must not reach a ws:// node through them.
    @Test
    fun `a ws node whose host forbids cleartext is never dialled`() = runBlocking {
        cleartextPermitted = false
        socketService.start(serve(node()), remainPaused = false)

        withTimeout(TIMEOUT) { socketService.networkStateFlow().first { it is State.WaitingForReconnect } }

        assertEquals(0, server.requestCount)
    }

    // SocketService disconnects a socket that is still dialling whenever it pauses or switches node.
    // The connection the dial opens afterwards must be released, or it stays open behind a socket
    // SocketService already counts as closed.
    @Test
    fun `a socket disconnected while dialling releases the connection it opens afterwards`() {
        val released = CountDownLatch(1)
        server.enqueue(
            MockResponse.Builder()
                .headersDelay(SLOW_HANDSHAKE_MILLIS, TimeUnit.MILLISECONDS)
                .webSocketUpgrade(releaseWatcher(released))
                .build()
        )
        server.start()
        val events = LinkedBlockingQueue<String>()
        val socket = ProviderWebSocketFactory.forNodes(ByteArray(GENESIS_SIZE)) { true }
            .createSocket(server.url("/").toString().replaceFirst("http", "ws"))
            .addListener(recorder(events))

        socket.connectAsynchronously()
        server.takeRequest(TIMEOUT.inWholeSeconds, TimeUnit.SECONDS)
        socket.disconnect()

        assertTrue(released.await(TIMEOUT.inWholeSeconds, TimeUnit.SECONDS))
        assertEquals(listOf("CONNECTING", "CLOSED"), events.toList())
    }

    // The light client finds its own peers, so the node URL SocketService picks must never be dialled.
    @Test
    fun `a chain on the light client is answered by it rather than the picked node`() = runBlocking {
        val lightClient = LightClient(storage = mockk(relaxed = true), chains = setOf(PREVIEWNET_RELAY))
        val socketService = SocketService(
            jsonMapper = gson,
            logger = WsConnectionLogger(ignoreStateMachineLogs = true),
            webSocketFactory = ProviderWebSocketFactory.forLightClient(lightClient, PREVIEWNET_RELAY.hexToByteArray()),
            reconnector = Reconnector(),
            requestExecutor = RequestExecutor(),
        )
        socketService.start(serve(node()), remainPaused = false)

        val response = withTimeout(TIMEOUT) {
            socketService.executeAsync(RuntimeRequest("chainSpec_v1_genesisHash", emptyList()))
        }
        socketService.stop()

        assertEquals("0x$PREVIEWNET_RELAY", response.result)
        assertEquals(0, server.requestCount)
    }

    private fun serve(listener: WebSocketListener): String {
        server.enqueue(upgrade(listener))
        server.start()
        return server.url("/").toString().replaceFirst("http", "ws")
    }

    private fun upgrade(listener: WebSocketListener) = MockResponse.Builder().webSocketUpgrade(listener).build()

    private fun recorder(events: LinkedBlockingQueue<String>) = object : WebSocketAdapter() {
        override fun onStateChanged(websocket: SdkWebSocket, newState: WebSocketState) {
            events.add(newState.name)
        }

        override fun onConnected(websocket: SdkWebSocket, headers: Map<String, List<String>>) {
            events.add("connected")
        }
    }

    private fun releaseWatcher(released: CountDownLatch) = object : WebSocketListener() {
        override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
            webSocket.close(code, null)
        }

        override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
            released.countDown()
        }

        override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
            released.countDown()
        }
    }

    // Answers each request with its method name; a subscription gets one notification.
    private fun node(dropAfterNotifying: Boolean = false) = object : WebSocketListener() {
        override fun onMessage(webSocket: WebSocket, text: String) {
            val request = gson.fromJson(text, JsonObject::class.java)
            val method = request["method"].asString
            webSocket.send("""{"jsonrpc":"2.0","id":${request["id"]},"result":"$method"}""")
            if (method != SUBSCRIBE) return

            webSocket.send(
                """{"jsonrpc":"2.0","method":"chain_newHead","params":{"subscription":"$SUBSCRIBE","result":"$HEAD"}}"""
            )
            if (dropAfterNotifying) webSocket.close(GOING_AWAY, null)
        }

        override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
            webSocket.close(code, null)
        }
    }

    private companion object {
        const val GENESIS_SIZE = 32
        const val SUBSCRIBE = "chain_subscribeNewHeads"
        const val HEAD = "0x01"
        const val GOING_AWAY = 1001
        const val SLOW_HANDSHAKE_MILLIS = 500L
        const val PREVIEWNET_RELAY = "48ab0689e87be8c502ace5f1c953baf4337daad39406f1e596c5f2fed5950fe6"
        val TIMEOUT = 10.seconds
    }
}
