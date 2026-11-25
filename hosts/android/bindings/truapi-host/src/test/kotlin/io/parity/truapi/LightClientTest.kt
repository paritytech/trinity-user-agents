package io.parity.truapi

import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi_provider.StorageClient
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit

// Chain-spec queries are answered from the bundled spec before the chain syncs, so these run offline.
class LightClientTest {
    private val loads = LinkedBlockingQueue<String>()
    private val responses = LinkedBlockingQueue<String>()
    private val closed = LinkedBlockingQueue<UInt>()

    private val storage = object : StorageClient {
        override suspend fun load(genesisHash: ByteArray, stack: String?): String? {
            loads.add(genesisHash.toHexString())
            return null
        }

        override suspend fun save(genesisHash: ByteArray, blob: String, stack: String?) = Unit
    }

    private fun provider(urls: List<String>) = CoreChainConnections(
        resolver = { urls },
        lightClient = LightClient(storage, setOf(PREVIEWNET_RELAY)),
    ).apply {
        attach(onResponse = { _, frame -> responses.add(frame) }, onClosed = { closed.add(it) })
    }

    private fun <T> LinkedBlockingQueue<T>.next(): T? = poll(30, TimeUnit.SECONDS)

    // A chain the light client serves must not reach the node the resolver names: the point of the
    // light client is that no single node is trusted with it.
    @Test
    fun `a chain the light client serves is answered by it rather than the named node`() {
        val provider = provider(listOf("ws://127.0.0.1:1/"))

        val id = provider.connect(PREVIEWNET_RELAY.hexToByteArray())!!
        provider.send(id, GENESIS_REQUEST)

        assertEquals("""{"jsonrpc":"2.0","id":1,"result":"0x$PREVIEWNET_RELAY"}""", responses.next())
        assertEquals(PREVIEWNET_RELAY, loads.next())
        assertEquals(null, closed.poll(1, TimeUnit.SECONDS))
    }

    @Test
    fun `a chain the light client serves connects without any node`() {
        val provider = provider(emptyList())

        val id = provider.connect(PREVIEWNET_RELAY.hexToByteArray())

        assertEquals(true, id != null)
    }

    @Test
    fun `a chain the light client does not serve still needs a node`() {
        val provider = provider(emptyList())

        assertEquals(null, provider.connect(ByteArray(32)))
    }

    private companion object {
        const val PREVIEWNET_RELAY = "48ab0689e87be8c502ace5f1c953baf4337daad39406f1e596c5f2fed5950fe6"
        const val GENESIS_REQUEST = """{"jsonrpc":"2.0","id":1,"method":"chainSpec_v1_genesisHash","params":[]}"""
    }
}
