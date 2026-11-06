package io.parity.truapi

import mockwebserver3.MockResponse
import mockwebserver3.MockWebServer
import okhttp3.OkHttpClient
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import org.junit.Assert.assertEquals
import org.junit.Test
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit

class WebSocketChainProviderObserverTest {
    private val seen = LinkedBlockingQueue<String>()
    private val client = OkHttpClient()
    private val genesis = ByteArray(32) { 0xab.toByte() }

    private val observer = object : ChainConnectionObserver {
        override fun dialling(connectionId: UInt, genesisHash: ByteArray, attempt: Int) {
            seen.add("dialling $connectionId attempt $attempt ${genesisHash.contentEquals(genesis)}")
        }

        override fun closingByHost(connectionId: UInt) {
            seen.add("closing $connectionId by host")
        }
    }

    private fun provider(urls: List<String>) = WebSocketChainProvider(
        resolver = { urls },
        client = client,
        onLog = { line -> if (line.contains(" open on ")) seen.add("open") },
        observer = observer,
    ).apply { attach(onResponse = { _, _ -> }, onClosed = { seen.add("closed $it") }) }

    private val echoesClose = object : WebSocketListener() {
        override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
            webSocket.close(code, null)
        }
    }

    private fun next(): String? = seen.poll(5, TimeUnit.SECONDS)

    @Test
    fun `the first dial is announced before any socket opens, and each dead node's replacement after it`() {
        MockWebServer().use { server ->
            server.enqueue(MockResponse.Builder().webSocketUpgrade(object : WebSocketListener() {}).build())
            server.start()
            val refused = "ws://127.0.0.1:1/"
            val answering = server.url("/").toString().replaceFirst("http", "ws")

            provider(listOf(refused, answering)).connect(genesis)

            assertEquals(
                listOf("dialling 1 attempt 0 true", "dialling 1 attempt 1 true", "open"),
                listOf(next(), next(), next()),
            )
        }
    }

    @Test
    fun `a host close names each connection before the core hears it closed`() {
        MockWebServer().use { server ->
            server.enqueue(MockResponse.Builder().webSocketUpgrade(echoesClose).build())
            server.start()
            val provider = provider(listOf(server.url("/").toString().replaceFirst("http", "ws")))
            provider.connect(genesis)
            assertEquals(listOf("dialling 1 attempt 0 true", "open"), listOf(next(), next()))

            provider.closeAll()

            assertEquals(listOf("closing 1 by host", "closed 1"), listOf(next(), next()))
        }
    }

    @Test
    fun `a close the core asked for is not a host close`() {
        MockWebServer().use { server ->
            server.enqueue(MockResponse.Builder().webSocketUpgrade(echoesClose).build())
            server.start()
            val provider = provider(listOf(server.url("/").toString().replaceFirst("http", "ws")))
            val id = checkNotNull(provider.connect(genesis))
            assertEquals(listOf("dialling 1 attempt 0 true", "open"), listOf(next(), next()))

            provider.close(id)
            provider.closeAll()

            assertEquals(null, seen.poll(500, TimeUnit.MILLISECONDS))
        }
    }
}
