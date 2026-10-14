package io.parity.truapi

import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicReference

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
 * Opens one OkHttp WebSocket per `chainConnect`, forwards `chainSend` frames,
 * and delivers inbound events to the callbacks set via [attach].
 *
 * An endpoint that fails before it ever opens is treated as a dead node rather
 * than a dead connection: the next candidate is tried under the same connection
 * id, so the core sees one connection attempt regardless of how many nodes it
 * took. Once a socket has opened, a later failure is a genuine close and is
 * reported as such.
 *
 * Chain calls arrive on the caller's dispatcher thread while OkHttp delivers
 * socket events on its own, so deliveries are guarded: a callback racing with
 * [detach] must not crash OkHttp's thread.
 */
class WebSocketChainProvider(
    private val resolver: ChainEndpointResolver,
    private val client: OkHttpClient,
    // This module depends only on jna, coroutines and okhttp so the host shell
    // stays publishable, so diagnostics leave through the host's own logger
    // rather than pulling one in here.
    private val onLog: (message: String) -> Unit = {},
) {
    private class EventSink(
        val onResponse: (connectionId: UInt, frame: String) -> Unit,
        val onClosed: (connectionId: UInt) -> Unit,
    )

    private val nextId = AtomicInteger(1)
    private val connections = ConcurrentHashMap<UInt, ChainConnection>()

    // Set via attach() rather than the constructor because the consumer usually
    // exists only after the provider. Cleared by detach() so socket callbacks
    // that race with teardown stop being delivered.
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
     * Stops delivering socket events. Call together with [closeAll] during
     * teardown, before the consumer is closed.
     */
    fun detach() {
        this.sink = null
    }

    /** Opens a connection for [genesisHash]. Null when the genesis is unsupported. */
    fun connect(genesisHash: ByteArray): UInt? {
        val urls = resolver.urlsFor(genesisHash)
        if (urls.isEmpty()) return null

        val id = nextId.getAndIncrement().toUInt()
        val connection = ChainConnection(id, urls)
        // Registered before dialling so a terminal callback that fires during
        // dial() finds the entry and removes it, instead of racing the insert.
        connections[id] = connection
        connection.connectNextEndpoint()
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
        connections.keys.toList().forEach { id ->
            connections.remove(id)?.close(notifyCore = true)
        }
    }

    // A callback can still throw if its consumer was destroyed between the
    // sink read and the call (a closed NativeTrUApiCore throws
    // IllegalStateException), so swallow rather than crash OkHttp's thread.
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
     * One logical chain connection, which may outlive several sockets while it
     * works down [urls] looking for a node that answers.
     */
    private inner class ChainConnection(
        private val id: UInt,
        private val urls: List<String>,
    ) : WebSocketListener() {
        private val nextCandidate = AtomicInteger(0)

        // One atomic rather than separate opened/finished flags: onFailure has
        // to tell "never opened" from "closed underneath us" in a single read,
        // or a close landing between two reads is not observed.
        private val state = AtomicReference(State.CONNECTING)

        @Volatile
        private var socket: WebSocket? = null

        /**
         * Opens the next candidate endpoint. False when the list is exhausted
         * or the connection already finished.
         *
         * [closeAll] runs off the core's thread, so a close can land mid-connect:
         * either before the socket exists, or after this method checked.
         */
        fun connectNextEndpoint(): Boolean {
            if (state.get() == State.FINISHED) return false
            val url = urls.getOrNull(nextCandidate.getAndIncrement()) ?: return false
            val opening = client.newWebSocket(Request.Builder().url(url).build(), this)
            socket = opening
            if (state.get() == State.FINISHED) {
                opening.close(NORMAL_CLOSURE, null)
                return false
            }
            return true
        }

        fun send(request: String) {
            if (state.get() == State.FINISHED) return
            socket?.send(request)
        }

        /**
         * @param notifyCore whether to tell the core the connection is gone.
         * False when the core asked for the close, true when the host decided.
         */
        fun close(notifyCore: Boolean) {
            if (state.getAndSet(State.FINISHED) == State.FINISHED) return
            socket?.close(NORMAL_CLOSURE, null)
            if (notifyCore) notifyClosed(id)
        }

        override fun onOpen(webSocket: WebSocket, response: Response) {
            state.compareAndSet(State.CONNECTING, State.OPEN)
        }

        override fun onMessage(webSocket: WebSocket, text: String) {
            // Frames keep arriving through a graceful close, and the core has
            // already been told this id is gone.
            if (state.get() == State.FINISHED) return
            notifyResponse(id, text)
        }

        override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
            finish()
        }

        override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
            // Only a failure while still connecting is a dead node worth
            // replacing: anything else is a real close.
            if (state.get() == State.CONNECTING && connectNextEndpoint()) return
            finish()
        }

        private fun finish() {
            if (state.getAndSet(State.FINISHED) == State.FINISHED) return
            connections.remove(id)
            notifyClosed(id)
        }
    }

    private enum class State { CONNECTING, OPEN, FINISHED }

    private companion object {
        /** WebSocket normal-closure status code (RFC 6455). */
        const val NORMAL_CLOSURE = 1000
    }
}
