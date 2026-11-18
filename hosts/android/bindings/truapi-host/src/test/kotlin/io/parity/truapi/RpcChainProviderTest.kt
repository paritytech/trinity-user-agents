package io.parity.truapi

import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit

class RpcChainProviderTest {
    private val server = MockWebServer()
    private val lines = LinkedBlockingQueue<String>()
    private val responses = LinkedBlockingQueue<Pair<UInt, String>>()
    private val closed = LinkedBlockingQueue<UInt>()

    @After
    fun tearDown() {
        server.close()
    }

    private fun provider(
        urls: List<String>,
        cleartextPermitted: (host: String) -> Boolean = { true },
    ) = RpcChainProvider(
        resolver = { urls },
        cleartextPermitted = cleartextPermitted,
        onLog = { lines.add(it) },
    ).apply {
        attach(
            onResponse = { id, frame -> responses.add(id to frame) },
            onClosed = { closed.add(it) },
        )
    }

    private fun serve(listener: WebSocketListener): String {
        server.enqueue(MockResponse.Builder().webSocketUpgrade(listener).build())
        server.start()
        return server.url("/").toString().replaceFirst("http", "ws")
    }

    private val echo = object : WebSocketListener() {
        override fun onMessage(webSocket: WebSocket, text: String) {
            webSocket.send(text)
        }

        override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
            webSocket.close(code, null)
        }
    }

    private fun <T> LinkedBlockingQueue<T>.next(): T? = poll(10, TimeUnit.SECONDS)

    @Test
    fun `an unsupported chain is refused and logs its genesis`() {
        val connectionId = provider(emptyList()).connect(ByteArray(32) { 0xab.toByte() })

        assertNull(connectionId)
        assertEquals(listOf("chain ${"ab".repeat(32)} has no host-managed connection; unsupported"), lines.toList())
    }

    // The core sends its first request right after chainConnect returns, before
    // the node has answered: dropping it would hang that request forever.
    @Test
    fun `a request sent while dialling reaches the node once it opens`() {
        val url = serve(echo)
        val provider = provider(listOf(url))

        val id = provider.connect(ByteArray(32))!!
        provider.send(id, REQUEST)

        assertEquals(id to REQUEST, responses.next())
    }

    @Test
    fun `a refused endpoint falls back to the next under the same id`() {
        val refused = "ws://127.0.0.1:1/"
        val answering = serve(echo)
        val provider = provider(listOf(refused, answering))

        val id = provider.connect(ByteArray(32))!!
        provider.send(id, REQUEST)

        assertEquals(id to REQUEST, responses.next())
        assertEquals("chain connection $id dialling $refused", lines.next())
        assertTrue(lines.next().orEmpty().startsWith("chain connection $id failed on $refused: "))
        assertEquals(
            listOf("chain connection $id dialling $answering", "chain connection $id open on $answering"),
            listOf(lines.next(), lines.next()),
        )
        assertTrue(closed.isEmpty())
    }

    @Test
    fun `the core learns of a connection no endpoint answered`() {
        val id = provider(listOf("ws://127.0.0.1:1/")).connect(ByteArray(32))

        assertEquals(id, closed.next())
    }

    @Test
    fun `the core learns of a node dropping the socket`() {
        val url = serve(object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                webSocket.cancel()
            }
        })

        val id = provider(listOf(url)).connect(ByteArray(32))

        assertEquals(id, closed.next())
    }

    // closeAll is the host's own decision (background, network switch), so the
    // core must hear of it to re-dial; close is the core's, so it already knows.
    @Test
    fun `only a host-initiated close is reported to the core`() {
        val url = serve(echo)
        server.enqueue(MockResponse.Builder().webSocketUpgrade(echo).build())
        val provider = provider(listOf(url))

        val askedByCore = provider.connect(ByteArray(32))!!
        val closedByHost = provider.connect(ByteArray(32))!!
        provider.send(askedByCore, REQUEST)
        provider.send(closedByHost, REQUEST)
        repeat(2) { responses.next() }

        provider.close(askedByCore)
        provider.closeAll()

        assertEquals(closedByHost, closed.next())
        assertNull(closed.poll(1, TimeUnit.SECONDS))
    }

    // The provider's sockets bypass the app's network security config, so a
    // release build that forbids cleartext must not reach a ws:// node anyway.
    @Test
    fun `a ws endpoint whose host forbids cleartext is skipped for the next one`() {
        val forbidden = "ws://forbidden.example/"
        val answering = serve(echo)
        val provider = provider(listOf(forbidden, answering), cleartextPermitted = { it != "forbidden.example" })

        val id = provider.connect(ByteArray(32))!!
        provider.send(id, REQUEST)

        assertEquals(id to REQUEST, responses.next())
        assertEquals(
            "chain connection $id skipped $forbidden: cleartext traffic is not permitted to its host",
            lines.next(),
        )
        assertEquals("chain connection $id dialling $answering", lines.next())
    }

    @Test
    fun `the core learns of a connection whose only endpoint forbids cleartext`() {
        val id = provider(listOf("ws://forbidden.example/"), cleartextPermitted = { false }).connect(ByteArray(32))

        assertEquals(id, closed.next())
    }

    @Test
    fun `a wss endpoint is not checked against the cleartext policy`() {
        val checked = LinkedBlockingQueue<String>()

        val id = provider(listOf("wss://127.0.0.1:1/"), cleartextPermitted = { checked.add(it) }).connect(ByteArray(32))

        assertEquals(id, closed.next())
        assertTrue(checked.isEmpty())
    }

    private companion object {
        const val REQUEST = """{"jsonrpc":"2.0","id":1,"method":"system_health","params":[]}"""
    }
}
