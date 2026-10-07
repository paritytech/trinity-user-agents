package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString
import okio.ByteString.Companion.toByteString
import uniffi.truapi.WsBridgeEndpoint
import javax.inject.Inject

/** One open connection to an execution's loopback bridge. */
interface WorkerBridgeConnection {
    fun send(frame: ByteArray)
    fun close()
}

/** What the bridge tells the runtime. Called on the transport's threads. */
interface WorkerBridgeEvents {
    fun onFrame(frame: ByteArray)
    fun onClosed(code: Int, reason: String)
    fun onFailure(cause: Throwable)
}

/** Opens the WebSocket an execution's `startWsBridge` describes; a test substitutes a fake. */
interface WorkerBridgeConnector {
    fun connect(endpoint: WsBridgeEndpoint, events: WorkerBridgeEvents): WorkerBridgeConnection
}

class OkHttpWorkerBridgeConnector @Inject constructor(
    private val client: OkHttpClient,
) : WorkerBridgeConnector {
    override fun connect(endpoint: WsBridgeEndpoint, events: WorkerBridgeEvents): WorkerBridgeConnection {
        val request = Request.Builder().url("ws://127.0.0.1:${endpoint.port}/?t=${endpoint.token}").build()
        val socket = client.newWebSocket(
            request,
            object : WebSocketListener() {
                override fun onMessage(webSocket: WebSocket, bytes: ByteString) = events.onFrame(bytes.toByteArray())
                override fun onClosed(webSocket: WebSocket, code: Int, reason: String) = events.onClosed(code, reason)
                override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) = events.onFailure(t)
            },
        )
        return object : WorkerBridgeConnection {
            override fun send(frame: ByteArray) {
                socket.send(frame.toByteString())
            }

            override fun close() {
                socket.close(NORMAL_CLOSURE, null)
            }
        }
    }

    private companion object {
        const val NORMAL_CLOSURE = 1000
    }
}
