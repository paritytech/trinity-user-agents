package io.parity.truapi

import uniffi.truapi_provider.ChainCloseReason
import uniffi.truapi_provider.ChainMessageListener
import uniffi.truapi_provider.ChainProvider
import uniffi.truapi_provider.ChainProviderException
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicInteger
import uniffi.truapi_provider.ChainConnection as RpcConnection

/**
 * Resolves a 32-byte chain genesis hash to its JSON-RPC WebSocket endpoints, in
 * preference order. Return an empty list for an unknown genesis; the core
 * surfaces that as a per-chain failure.
 *
 * Called on the core's dispatcher thread, so implementations must answer from
 * memory and never block.
 */
fun interface ChainEndpointResolver {
    fun urlsFor(genesisHash: ByteArray): List<String>
}

/**
 * Serves the core's `chainConnect`, `chainSend` and `chainClose` through
 * [ChainProvider.connectRpc], delivering inbound events to the callbacks set via
 * [attach].
 *
 * An endpoint that fails before it ever opens is treated as a dead node rather
 * than a dead connection: the next candidate is tried under the same connection
 * id, so the core sees one connection attempt regardless of how many nodes it
 * took. Once a socket has opened, a later close is reported as such; the
 * provider never reconnects on its own.
 */
class RpcChainProvider(
    private val resolver: ChainEndpointResolver,
    private val provider: ChainProvider,
    private val onLog: (message: String) -> Unit = {},
) {
    private class EventSink(
        val onResponse: (connectionId: UInt, frame: String) -> Unit,
        val onClosed: (connectionId: UInt) -> Unit,
    )

    private val nextId = AtomicInteger(1)
    private val connections = ConcurrentHashMap<UInt, Connection>()

    // Set via attach() rather than the constructor because the consumer usually
    // exists only after the provider. Cleared by detach() so callbacks that race
    // with teardown stop being delivered.
    @Volatile
    private var sink: EventSink? = null

    /** Sets where inbound frames and closes go. Call before any chain traffic. */
    fun attach(
        onResponse: (connectionId: UInt, frame: String) -> Unit,
        onClosed: (connectionId: UInt) -> Unit,
    ) {
        this.sink = EventSink(onResponse, onClosed)
    }

    /**
     * Stops delivering connection events. Call together with [closeAll] during
     * teardown, before the consumer is closed.
     */
    fun detach() {
        this.sink = null
    }

    /**
     * Opens a connection for [genesisHash] and returns its id at once; the node
     * is dialled in the background and requests sent meanwhile are held until it
     * answers. Null when the genesis is unsupported.
     */
    fun connect(genesisHash: ByteArray): UInt? {
        val urls = resolver.urlsFor(genesisHash)
        if (urls.isEmpty()) {
            onLog("chain ${genesisHash.toHexString()} has no host-managed connection; unsupported")
            return null
        }

        val id = nextId.getAndIncrement().toUInt()
        val connection = Connection(id, urls)
        // Registered before dialling so a close that lands mid-dial finds it.
        connections[id] = connection
        dialler.execute(connection::dial)
        return id
    }

    fun send(connectionId: UInt, request: String) {
        val connection = connections[connectionId]
        if (connection == null) {
            onLog("chainSend to unknown/closed connection $connectionId")
            return
        }
        connection.send(request)
    }

    /** Closes a connection the core asked to close; the core already knows. */
    fun close(connectionId: UInt) {
        connections.remove(connectionId)?.close(notifyCore = false)
    }

    /**
     * Closes every connection on the host's initiative, e.g. going to
     * background. Unlike [close] this reports each one back to the core, which
     * would otherwise keep treating them as live and never re-dial.
     */
    fun closeAll() {
        if (connections.isNotEmpty()) onLog("closing ${connections.size} chain connections")
        connections.keys.toList().forEach { id ->
            connections.remove(id)?.close(notifyCore = true)
        }
    }

    // A callback can still throw if its consumer was destroyed between the
    // sink read and the call (a closed NativeTrUApiCore throws
    // IllegalStateException), so swallow rather than fail the provider's pump.
    private fun notifyResponse(id: UInt, text: String) {
        val s = sink ?: return
        runCatching { s.onResponse(id, text) }
            .onFailure { onLog("onResponse after teardown: $it") }
    }

    private fun notifyClosed(id: UInt) {
        val s = sink ?: return
        runCatching { s.onClosed(id) }
            .onFailure { onLog("onClosed after teardown: $it") }
    }

    /**
     * One logical chain connection, which may try several of [urls] before one
     * answers. Sends arrive on the core's thread while dials and inbound events
     * run on others, so all state changes happen under [lock].
     */
    private inner class Connection(
        private val id: UInt,
        private val urls: List<String>,
    ) : ChainMessageListener {
        private val lock = Any()
        private var state = State.CONNECTING
        private var socket: RpcConnection? = null
        private val pending = ArrayList<String>()

        @Volatile
        private var dialledUrl: String? = null

        fun dial() {
            for (url in urls) {
                if (synchronized(lock) { state == State.FINISHED }) return
                dialledUrl = url
                onLog("chain connection $id dialling $url")
                val opened = try {
                    provider.connectRpc(url, this)
                } catch (error: ChainProviderException) {
                    onLog("chain connection $id failed on $url: ${error.describe()}")
                    continue
                }
                onLog("chain connection $id open on $url")
                if (!adopt(opened)) {
                    opened.disconnect()
                    opened.destroy()
                }
                return
            }
            finish()
        }

        private fun adopt(opened: RpcConnection): Boolean = synchronized(lock) {
            if (state == State.FINISHED) return false
            state = State.OPEN
            socket = opened
            pending.forEach(opened::send)
            pending.clear()
            true
        }

        fun send(request: String) = synchronized(lock) {
            when (state) {
                State.CONNECTING -> pending.add(request)
                State.OPEN -> socket?.send(request)
                State.FINISHED -> Unit
            }
        }

        /**
         * @param notifyCore whether to tell the core the connection is gone.
         * False when the core asked for the close, true when the host decided.
         */
        fun close(notifyCore: Boolean) {
            val closing = markFinished() ?: return
            closing.socket?.let { socket ->
                socket.disconnect()
                socket.destroy()
            }
            if (notifyCore) notifyClosed(id)
        }

        override fun onMessage(message: String) {
            if (synchronized(lock) { state == State.FINISHED }) return
            notifyResponse(id, message)
        }

        override fun onClosed(reason: ChainCloseReason) {
            onLog("chain connection $id closed on $dialledUrl: ${reason.describe()}")
            finish()
        }

        private fun finish() {
            val finished = markFinished() ?: return
            connections.remove(id)
            finished.socket?.destroy()
            notifyClosed(id)
        }

        /** Null when the connection had already finished. */
        private fun markFinished(): Finished? = synchronized(lock) {
            if (state == State.FINISHED) return null
            state = State.FINISHED
            pending.clear()
            Finished(socket).also { socket = null }
        }
    }

    private class Finished(val socket: RpcConnection?)

    private enum class State { CONNECTING, OPEN, FINISHED }

    private companion object {
        // connectRpc blocks until the node answers or fails, and chainConnect is
        // called on the core's dispatcher thread, which must never block.
        val dialler: ExecutorService = Executors.newCachedThreadPool { task ->
            Thread(task, "truapi-chain-dial").apply { isDaemon = true }
        }
    }
}

private fun ChainProviderException.describe(): String =
    (this as? ChainProviderException.Connect)?.reason ?: javaClass.simpleName

private fun ChainCloseReason.describe(): String = when (this) {
    ChainCloseReason.StreamEnded -> "stream ended"
    is ChainCloseReason.ListenerFailed -> "listener failed: $reason"
}
