package io.parity.truapi

import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import okhttp3.OkHttpClient
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit

// The iOS host logs the same chain lines, so both hosts read alike in Sentry.
class WebSocketChainProviderLogTest {
    private val lines = LinkedBlockingQueue<String>()
    private val client = OkHttpClient()

    private fun provider(urls: List<String>) = WebSocketChainProvider(
        resolver = { urls },
        client = client,
        onLog = { lines.add(it) },
    ).apply { attach(onResponse = { _, _ -> }, onClosed = {}) }

    private fun nextLine(): String? = lines.poll(5, TimeUnit.SECONDS)

    private fun echoesClose(closeReason: String?) = object : WebSocketListener() {
        override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
            webSocket.close(code, closeReason)
        }
    }

    @Test
    fun `an unsupported chain logs its genesis`() {
        val connectionId = provider(emptyList()).connect(ByteArray(32) { 0xab.toByte() })

        assertNull(connectionId)
        assertEquals(listOf("chain ${"ab".repeat(32)} has no host-managed connection; unsupported"), lines.toList())
    }

    @Test
    fun `a connection logs its dial, its open and a host close`() {
        MockWebServer().use { server ->
            server.enqueue(MockResponse.Builder().webSocketUpgrade(echoesClose(closeReason = null)).build())
            server.start()
            val url = server.url("/").toString().replaceFirst("http", "ws")
            val provider = provider(listOf(url))

            provider.connect(ByteArray(32))
            val opened = listOf(nextLine(), nextLine())
            provider.closeAll()

            assertEquals(
                listOf("chain connection 1 dialling $url", "chain connection 1 open on $url"),
                opened,
            )
            assertEquals("closing 1 chain connections", nextLine())
            assertEquals("chain connection 1 closed on $url: 1000", nextLine())
        }
    }

    @Test
    fun `a close reason from the peer follows the code`() {
        MockWebServer().use { server ->
            server.enqueue(MockResponse.Builder().webSocketUpgrade(echoesClose(closeReason = "going away")).build())
            server.start()
            val url = server.url("/").toString().replaceFirst("http", "ws")
            val provider = provider(listOf(url))

            provider.connect(ByteArray(32))
            val opened = listOf(nextLine(), nextLine())
            provider.closeAll()

            assertEquals(
                listOf("chain connection 1 dialling $url", "chain connection 1 open on $url"),
                opened,
            )
            assertEquals("closing 1 chain connections", nextLine())
            assertEquals("chain connection 1 closed on $url: 1000 going away", nextLine())
        }
    }

    @Test
    fun `a refused dial logs a failure on its endpoint`() {
        val url = "ws://127.0.0.1:1/"

        provider(listOf(url)).connect(ByteArray(32))

        assertEquals("chain connection 1 dialling $url", nextLine())
        val failure = nextLine().orEmpty()
        assertTrue(failure, failure.startsWith("chain connection 1 failed on $url: ConnectException: "))
    }

    @Test
    fun `a failure without a message logs only its type`() {
        MockWebServer().use { server ->
            val dropsTheSocket = object : WebSocketListener() {
                override fun onOpen(webSocket: WebSocket, response: Response) {
                    webSocket.cancel()
                }
            }
            server.enqueue(MockResponse.Builder().webSocketUpgrade(dropsTheSocket).build())
            server.start()
            val url = server.url("/").toString().replaceFirst("http", "ws")

            provider(listOf(url)).connect(ByteArray(32))

            assertEquals(
                listOf(
                    "chain connection 1 dialling $url",
                    "chain connection 1 open on $url",
                    "chain connection 1 failed on $url: EOFException",
                ),
                listOf(nextLine(), nextLine(), nextLine()),
            )
        }
    }

    @Test
    fun `a refused endpoint falls back and names each url`() {
        MockWebServer().use { server ->
            server.enqueue(MockResponse.Builder().webSocketUpgrade(object : WebSocketListener() {}).build())
            server.start()
            val refused = "ws://127.0.0.1:1/"
            val answering = server.url("/").toString().replaceFirst("http", "ws")

            provider(listOf(refused, answering)).connect(ByteArray(32))

            assertEquals("chain connection 1 dialling $refused", nextLine())
            val failure = nextLine().orEmpty()
            assertTrue(failure, failure.startsWith("chain connection 1 failed on $refused: ConnectException"))
            assertEquals(
                listOf("chain connection 1 dialling $answering", "chain connection 1 open on $answering"),
                listOf(nextLine(), nextLine()),
            )
        }
    }
}
