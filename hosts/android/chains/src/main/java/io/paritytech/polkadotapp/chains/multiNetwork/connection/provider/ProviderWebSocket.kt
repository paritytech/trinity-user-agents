package io.paritytech.polkadotapp.chains.multiNetwork.connection.provider

import com.neovisionaries.ws.client.DetachedWebSocket
import com.neovisionaries.ws.client.WebSocket
import com.neovisionaries.ws.client.WebSocketFactory
import io.parity.truapi.CleartextNotPermittedException
import io.parity.truapi.describe
import io.parity.truapi.dialRpcNode
import io.parity.truapi.systemPermitsCleartext
import timber.log.Timber
import uniffi.truapi_provider.ChainCloseReason
import uniffi.truapi_provider.ChainConnection
import uniffi.truapi_provider.ChainMessageListener
import uniffi.truapi_provider.ChainProviderException
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors

class ProviderWebSocketFactory(
    private val genesisHash: ByteArray,
    private val cleartextPermitted: (host: String) -> Boolean = ::systemPermitsCleartext,
) : WebSocketFactory() {
    override fun createSocket(uri: String): WebSocket = ProviderWebSocket(this, genesisHash, uri, cleartextPermitted)

    override fun createSocket(uri: String, timeout: Int): WebSocket = createSocket(uri)
}

// The provider never reconnects: a failed dial or a dropped socket ends in CLOSED, so SocketService
// reconnects as it does for any node.
private class ProviderWebSocket(
    factory: WebSocketFactory,
    private val genesisHash: ByteArray,
    private val url: String,
    private val cleartextPermitted: (host: String) -> Boolean,
) : DetachedWebSocket(factory), ChainMessageListener {
    private val lock = Any()
    private var connection: ChainConnection? = null
    private var finished = false

    override fun connectAsynchronously(): WebSocket {
        reportConnecting()
        dialler.execute(::dial)
        return this
    }

    override fun sendText(message: String): WebSocket {
        synchronized(lock) { connection?.send(message) }
        return this
    }

    override fun disconnect(closeCode: Int, reason: String?, closeDelay: Long): WebSocket {
        finish()?.connection?.let { closing ->
            closing.disconnect()
            closing.destroy()
        }
        reportClosed()
        return this
    }

    override fun onMessage(message: String) {
        reportText(message)
    }

    override fun onClosed(reason: ChainCloseReason) {
        val finishing = finish() ?: return
        Timber.d("Provider connection to $url closed: ${reason.describe()}")
        finishing.connection?.destroy()
        reportClosed()
    }

    private fun dial() {
        val opened = try {
            dialRpcNode(genesisHash, url, this, cleartextPermitted)
        } catch (error: CleartextNotPermittedException) {
            fail(error)
            return
        } catch (error: ChainProviderException) {
            fail(error)
            return
        }

        val adopted = synchronized(lock) {
            if (!finished) connection = opened
            !finished
        }
        if (adopted) {
            reportOpen()
        } else {
            opened.disconnect()
            opened.destroy()
        }
    }

    private fun fail(error: Exception) {
        synchronized(lock) { finished = true }
        reportConnectError(error)
    }

    // Null when the socket had already finished, so a connection is released once. The connection
    // inside is null while the dial is still running; the dial then releases what it opens.
    private fun finish(): Finishing? = synchronized(lock) {
        if (finished) return null
        finished = true
        Finishing(connection).also { connection = null }
    }

    private class Finishing(val connection: ChainConnection?)

    private companion object {
        // ChainProvider.connect blocks until the node answers or fails.
        val dialler: ExecutorService = Executors.newCachedThreadPool { task ->
            Thread(task, "chain-provider-dial").apply { isDaemon = true }
        }
    }
}
