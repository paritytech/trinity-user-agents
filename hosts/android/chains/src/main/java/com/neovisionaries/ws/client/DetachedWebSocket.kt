package com.neovisionaries.ws.client

// In nv-websocket-client's package because WebSocket's only constructor is package-private.
abstract class DetachedWebSocket(factory: WebSocketFactory) :
    WebSocket(factory, false, null, "localhost", "/", null) {
    protected fun reportConnecting() {
        moveTo(WebSocketState.CONNECTING)
    }

    protected fun reportOpen() {
        if (moveTo(WebSocketState.OPEN)) listenerManager.callOnConnected(emptyMap())
    }

    protected fun reportText(text: String) {
        if (state == WebSocketState.OPEN) listenerManager.callOnTextMessage(text)
    }

    protected fun reportConnectError(cause: Throwable) {
        if (moveTo(WebSocketState.CLOSED)) {
            listenerManager.callOnConnectError(WebSocketException(WebSocketError.SOCKET_CONNECT_ERROR, cause.message, cause))
        }
    }

    protected fun reportClosed() {
        moveTo(WebSocketState.CLOSED)
    }

    // A closed socket never reopens, so an open that loses a race with a close is dropped.
    private fun moveTo(next: WebSocketState): Boolean {
        val moved = synchronized(stateManager) {
            val current = stateManager.state
            val allowed = current != WebSocketState.CLOSED && current != next
            if (allowed) stateManager.state = next
            allowed
        }
        if (moved) listenerManager.callOnStateChanged(next)
        return moved
    }

    // The base finalizer closes the TCP socket it would have dialled; this class never dials one.
    override fun finalize() = Unit
}
